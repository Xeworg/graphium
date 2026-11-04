#!/usr/bin/env bash
#
# Instalador / desinstalador de Graphium (Linux).
#
#   ./install.sh [install]      Compila con cargo (release) e instala
#                               $HOME/.local/bin/graphium.
#   ./install.sh agent          Instala o actualiza la extensión en los
#                               agentes elegidos (interactivo en una tty;
#                               --env/--all sin tty).
#   ./install.sh update         Actualiza el binario y la extensión de los
#                               agentes que ya la tienen si hay un release
#                               más nuevo que lo instalado.
#   ./install.sh uninstall      Saca el binario y la extensión de los
#                               agentes que la tienen; los datos se
#                               conservan.
#   ./install.sh --help         Muestra esta ayuda.
#
# El mismo script viaja en cada release, así que funciona en dos modos:
#
#   desde el repositorio   compila el árbol de trabajo con cargo;
#   pipeado (standalone)   baja el binario publicado y lo verifica contra el
#                          checksum que publica el release.
#
# El modo lo decide el propio script, no el directorio de trabajo: un script
# pipeado desde la red no tiene archivo propio, así que siempre es el modo
# standalone. Ese modo necesita bash (el script usa sintaxis bash), curl,
# sha256sum y tar:
#
#   curl -fsSL https://github.com/Xeworg/graphium/releases/latest/download/install.sh | bash
#
# El binario de Graphium es una app gráfica (Tauri): en Linux necesita
# webkit2gtk-4.1 en tiempo de ejecución (PRD §RNF2). El instalador lo avisa
# si no lo encuentra, pero no lo instala.

set -euo pipefail

usage() {
  printf '%s\n' \
    'Uso: ./install.sh [install]' \
    '       ./install.sh agent [--env <name> ... | --all]' \
    '       ./install.sh update' \
    '       ./install.sh uninstall' \
    '       ./install.sh --help' \
    '' \
    'Instala graphium en $HOME/.local/bin. Corrido desde el repositorio' \
    'compila el árbol de trabajo con cargo; servido desde un release (o' \
    'pipeado desde la red) baja el binario publicado y verifica su checksum.' \
    'Correrlo de nuevo sobreescribe el binario y, si hay un release más' \
    'nuevo, actualiza.' \
    '' \
    'Comandos:' \
    '  install    Instala (el comando por omisión). Con un release más nuevo' \
    '             publicado, actualiza el binario instalado.' \
    '  agent      Instala la extensión de Graphium (extensions/pi/graphium)' \
    '             en el agente de IA. Detecta el host presente (pi, con' \
    '             PI_CODING_AGENT_DIR o ~/.pi/agent) y si ya está instalada;' \
    '             en una terminal pregunta si instalar o actualizar. Sin tty' \
    '             hay que pasar --env pi o --all.' \
    '  update     Si hay un release más nuevo que lo instalado, actualiza el' \
    '             binario y refresca la extensión en cada agente que ya la' \
    '             tiene. Antes del primer release avisa y no hace nada.' \
    '  uninstall  Saca el binario y la extensión de los agentes que la' \
    '             tienen. Los datos de exportes (~/.graphium) se conservan.' \
    '' \
    'Opciones de agent:' \
    '  --env <name>  Agente destino (repetible): pi.' \
    '  --all         Todos los hosts detectados.' \
    '' \
    'El binario de Graphium abre una ventana (Tauri): necesita' \
    'webkit2gtk-4.1 en tiempo de ejecución; el instalador lo avisa si falta.'
}

error() { printf 'install.sh: %s\n' "$*" >&2; }
die() {
  error "$*"
  exit 1
}

if [[ -z "${HOME:-}" ]]; then
  die "HOME no está definida; no se puede resolver el directorio de instalación"
fi
INSTALL_DIR="$HOME/.local/bin"
GRAPHIUM_BIN="$INSTALL_DIR/graphium"

# --- los releases publicados ------------------------------------------------
# El mismo script viaja en cada release. Estas constantes son el único lugar
# que sabe dónde viven los releases.
REPO_URL="https://github.com/Xeworg/graphium"
RELEASES_URL="https://github.com/Xeworg/graphium/releases"

# Resuelve la raíz del repositorio sin herramientas externas para que el
# chequeo de toolchain funcione aun con un PATH vacío.
# Un script pipeado desde la red no tiene archivo propio y BASH_SOURCE queda
# vacío: leerlo sin proteger abortaría bajo `set -u`.
SCRIPT_PATH="${BASH_SOURCE[0]:-}"
SCRIPT_DIR="${SCRIPT_PATH%/*}"
if [[ -z "$SCRIPT_PATH" || "$SCRIPT_DIR" == "$SCRIPT_PATH" ]]; then
  SCRIPT_DIR="."
fi
SCRIPT_DIR="$(cd -- "$SCRIPT_DIR" && pwd -P)"
# El modo lo decide el script, no el directorio: un script pipeado no tiene
# archivo propio y el repositorio tiene su Cargo.toml en la raíz.
IN_REPO=0
if [[ -n "$SCRIPT_PATH" && -f "$SCRIPT_DIR/Cargo.toml" ]]; then
  IN_REPO=1
fi

require_rust_toolchain() {
  local missing=0
  if ! command -v cargo >/dev/null 2>&1; then
    error "cargo no fue encontrado en el PATH."
    missing=1
  fi
  if ! command -v rustc >/dev/null 2>&1; then
    error "rustc no fue encontrado en el PATH."
    missing=1
  fi
  if ((missing != 0)); then
    error "Instalá Rust con rustup (https://rustup.rs) y volvé a correr el script."
    exit 1
  fi
}

# Cargo resuelve un CARGO_TARGET_DIR relativo contra su directorio de trabajo,
# que acá es la raíz del repositorio.
cargo_target_dir() {
  local dir="${CARGO_TARGET_DIR:-$SCRIPT_DIR/target}"
  case "$dir" in
    /*) printf '%s\n' "$dir" ;;
    *) printf '%s\n' "$SCRIPT_DIR/$dir" ;;
  esac
}

# --- el instalador online ---------------------------------------------------

# La URL base del release del que se baja. Una versión vacía significa el
# último release publicado; una versión lo fija.
release_base_url() {
  local version="${1:-}"
  if [[ -n "$version" ]]; then
    printf '%s/download/v%s\n' "$RELEASES_URL" "${version#v}"
  else
    printf '%s/latest/download\n' "$RELEASES_URL"
  fi
}

# La versión del último release publicado, o nada si no hay releases (el
# enlace /latest responde 404 hasta que se publica el primero).
latest_release_version() {
  local url
  url="$(curl -fsSL -o /dev/null -w '%{url_effective}' "$RELEASES_URL/latest")" || return 1
  local tag="${url##*/tag/}"
  [[ -n "$tag" && "$tag" != "$url" ]] || return 1
  printf '%s\n' "${tag#v}"
}

# El slug de plataforma para el que los releases publican artefactos. El
# proyecto es Linux-first: cualquier otra cosa tiene que compilar desde
# fuente.
release_platform() {
  case "$(uname -s)" in
    Linux) ;;
    *) return 1 ;;
  esac
  case "$(uname -m)" in
    x86_64 | amd64) printf 'linux-x86_64' ;;
    aarch64 | arm64) printf 'linux-aarch64' ;;
    *) return 1 ;;
  esac
}

require_download_tools() {
  local missing=0
  command -v curl >/dev/null 2>&1 || {
    error "curl no fue encontrado en el PATH."
    missing=1
  }
  command -v sha256sum >/dev/null 2>&1 || {
    error "sha256sum no fue encontrado en el PATH."
    missing=1
  }
  command -v tar >/dev/null 2>&1 || {
    error "tar no fue encontrado en el PATH."
    missing=1
  }
  if ((missing != 0)); then
    error "Instalar un binario publicado necesita curl, sha256sum y tar."
    exit 1
  fi
}

# Baja un asset del release elegido y lo verifica contra el SHA256SUMS que
# publica el release. Deja la ruta del asset en ASSET_FILE y el directorio de
# trabajo en DOWNLOAD_DIR (el llamador borra DOWNLOAD_DIR).
download_verified() {
  local asset="$1" version="${2:-}"
  require_download_tools
  local base tmp expected actual
  base="$(release_base_url "$version")"
  tmp="$(mktemp -d)"
  DOWNLOAD_DIR="$tmp"
  ASSET_FILE="$tmp/$asset"

  printf 'Bajando %s/%s\n' "$base" "$asset"
  if ! curl -fsSL --retry 3 --retry-delay 1 -o "$ASSET_FILE" "$base/$asset"; then
    rm -rf -- "$tmp"
    error "la descarga falló: $base/$asset"
    if [[ -z "$version" ]]; then
      error "¿Hay un release publicado? El primer release aparece al etiquetar"
      error "(por ejemplo v0.1.0); hasta entonces, instalá desde el repositorio."
    fi
    return 1
  fi
  printf 'Bajando %s/SHA256SUMS\n' "$base"
  if ! curl -fsSL --retry 3 --retry-delay 1 -o "$tmp/SHA256SUMS" "$base/SHA256SUMS"; then
    rm -rf -- "$tmp"
    error "no se pudo bajar el checksum: $base/SHA256SUMS"
    return 1
  fi
  expected="$(awk -v name="$asset" '$2 == name {print $1}' "$tmp/SHA256SUMS")"
  if [[ -z "$expected" ]]; then
    rm -rf -- "$tmp"
    error "el release no publica checksum para $asset; no se instala."
    return 1
  fi
  actual="$(sha256sum -- "$ASSET_FILE" | awk '{print $1}')"
  if [[ "$actual" != "$expected" ]]; then
    rm -rf -- "$tmp"
    error "checksum inválido para $asset; no se instala."
    return 1
  fi
}

# La versión que reporta el binario instalado, o vacía si no hay ninguno.
# `graphium --version` imprime «graphium 0.1.0».
installed_version() {
  [[ -x "$GRAPHIUM_BIN" ]] || return 0
  "$GRAPHIUM_BIN" --version 2>/dev/null | awk '{print $NF}' || true
}

# La versión de una extensión instalada (package.json), o vacía si no está.
extension_installed_version() {
  local dir="$1"
  [[ -f "$dir/package.json" ]] || return 0
  sed -n 's/.*"version"[: ]*"\([^"]*\)".*/\1/p' "$dir/package.json" | head -1
}

# Devuelve verdadero (0) cuando $2 es más nueva que $1.
version_is_newer() {
  local installed="$1" candidate="$2"
  [[ -n "$installed" && -n "$candidate" ]] || return 1
  local oldest
  oldest="$(printf '%s\n%s\n' "$installed" "$candidate" | sort -V | head -1)"
  [[ "$oldest" == "$installed" && "$installed" != "$candidate" ]]
}

# Aviso no fatal: la app es gráfica y sin webkit2gtk-4.1 no abre la ventana.
warn_runtime_deps() {
  local found=0
  if command -v ldconfig >/dev/null 2>&1 && ldconfig -p 2>/dev/null | grep -q webkit2gtk-4.1; then
    found=1
  elif command -v pkg-config >/dev/null 2>&1 && pkg-config --exists webkit2gtk-4.1 2>/dev/null; then
    found=1
  fi
  if ((found == 0)); then
    printf 'Aviso: no se encontró webkit2gtk-4.1. Graphium abre una ventana y la\n'
    printf 'necesita en tiempo de ejecución; instalala con el gestor de paquetes de\n'
    printf 'tu distribución (por ejemplo: sudo apt install libwebkit2gtk-4.1-dev).\n'
  fi
}

# --- el binario -------------------------------------------------------------

# Instalación del binario desde el repositorio: compila y copia.
install_binary_from_source_repo() {
  require_rust_toolchain
  printf 'Compilando (cargo build --release)...\n'
  (cd -- "$SCRIPT_DIR" && cargo build --release)
  local binary
  binary="$(cargo_target_dir)/release/graphium"
  [[ -f "$binary" ]] || die "la compilación no produjo $binary"
  mkdir -p -- "$INSTALL_DIR"
  install -m 0755 -- "$binary" "$GRAPHIUM_BIN"
  printf 'Instalado %s\n' "$GRAPHIUM_BIN"
}

# Instalación del binario desde un release publicado: baja, verifica,
# extrae, instala. Falla (return 1) si no hay release o la descarga no
# verifica; el llamador decide si eso es fatal.
install_binary_from_release() {
  local version="${1:-}"
  local platform
  platform="$(release_platform)" || {
    error "graphium publica binarios para linux-x86_64 y linux-aarch64 solamente;"
    error "esta máquina reporta: $(uname -s) $(uname -m)."
    error "Alternativa: cloná $REPO_URL y corré ./install.sh desde el repositorio."
    return 1
  }
  download_verified "graphium-$platform.tar.gz" "$version" || return 1
  mkdir -p -- "$DOWNLOAD_DIR/extract"
  if ! tar -xzf "$ASSET_FILE" -C "$DOWNLOAD_DIR/extract"; then
    rm -rf -- "$DOWNLOAD_DIR"
    error "el archivo bajado no pudo extraerse."
    return 1
  fi
  mkdir -p -- "$INSTALL_DIR"
  install -m 0755 -- "$DOWNLOAD_DIR/extract/graphium" "$GRAPHIUM_BIN"
  rm -rf -- "$DOWNLOAD_DIR"
  printf 'Instalado %s\n' "$GRAPHIUM_BIN"
}

# Instala o actualiza el binario, eligiendo el modo según el script. Con
# announce_update en 1, un binario viejo instalado genera el aviso de
# actualización (el modo release baja el nuevo; el modo repo compila).
install_binary() {
  local announce_update="${1:-0}"
  local previous
  previous="$(installed_version)"
  if ((IN_REPO == 1)); then
    if [[ -n "$previous" ]] && version_is_newer "$previous" "$(git -C "$SCRIPT_DIR" describe --tags --abbrev=0 2>/dev/null | sed 's/^v//' || true)"; then
      printf 'Actualizando: %s -> ' "$previous"
    elif [[ -n "$previous" ]]; then
      printf 'Reinstalando (compilación del árbol de trabajo)...\n'
    fi
    install_binary_from_source_repo
  else
    local latest=""
    latest="$(latest_release_version 2>/dev/null || true)"
    if [[ -n "$previous" && -n "$latest" ]] && version_is_newer "$previous" "$latest"; then
      printf 'Actualizando: %s -> %s\n' "$previous" "$latest"
    fi
    install_binary_from_release ""
  fi
  if ((announce_update == 1)) && [[ -n "$previous" ]]; then
    printf 'La versión anterior era %s; ahora es %s.\n' "$previous" "$(installed_version)"
  fi
}

# --- la extensión de los agentes --------------------------------------------

# Los hosts de agente que el instalador conoce, y dónde busca extensiones
# cada uno. gentle-shell es un flavor de Pi y honra la misma variable: hay
# un solo host «pi», cuyo directorio de agente es PI_CODING_AGENT_DIR o
# ~/.pi/agent.
agent_envs() {
  printf '%s\n' pi
}

agent_env_dir() {
  case "$1" in
    pi) printf '%s\n' "${PI_CODING_AGENT_DIR:-$HOME/.pi/agent}/extensions" ;;
    *) return 1 ;;
  esac
}

# Un host «existe» cuando su directorio de agente está en el disco: no tiene
# sentido ofrecer un host que la máquina no usa.
agent_env_present() {
  local dir
  dir="$(agent_env_dir "$1")" || return 1
  [[ -d "$(dirname -- "$dir")" ]]
}

# Instala la extensión en un host desde el árbol del repositorio.
install_extension_from_repo() {
  local env="$1" dest
  dest="$(agent_env_dir "$env")"
  mkdir -p -- "$dest"
  rm -rf -- "$dest/graphium"
  cp -R -- "$SCRIPT_DIR/extensions/pi/graphium" "$dest/graphium"
  printf 'Extensión instalada en %s (graphium)\n' "$dest"
}

# Instala la extensión en un host desde un release publicado: la extensión
# viaja como asset (graphium-pi-extension.tar.gz, con el directorio graphium/
# adentro).
install_extension_from_release() {
  local env="$1" dest
  dest="$(agent_env_dir "$env")"
  download_verified "graphium-pi-extension.tar.gz" "" || return 1
  mkdir -p -- "$DOWNLOAD_DIR/extract"
  if ! tar -xzf "$ASSET_FILE" -C "$DOWNLOAD_DIR/extract"; then
    rm -rf -- "$DOWNLOAD_DIR"
    error "el asset de la extensión no pudo extraerse."
    return 1
  fi
  [[ -f "$DOWNLOAD_DIR/extract/graphium/index.ts" ]] || {
    rm -rf -- "$DOWNLOAD_DIR"
    error "el asset de la extensión no trae graphium/index.ts; no se instala."
    return 1
  }
  mkdir -p -- "$dest"
  rm -rf -- "$dest/graphium"
  cp -R -- "$DOWNLOAD_DIR/extract/graphium" "$dest/graphium"
  rm -rf -- "$DOWNLOAD_DIR"
  printf 'Extensión instalada en %s (graphium)\n' "$dest"
}

install_extension() {
  local env="$1"
  if ((IN_REPO == 1)); then
    install_extension_from_repo "$env"
  else
    install_extension_from_release "$env"
  fi
}

# Línea de estado de un host: «presente»/«no presente», versión instalada.
agent_env_status() {
  local env="$1" dest present="ausente" version=""
  if agent_env_present "$env"; then
    present="presente"
    version="$(extension_installed_version "$(agent_env_dir "$env")/graphium")"
  fi
  if [[ -n "$version" ]]; then
    printf '%s: host %s, extensión instalada v%s\n' "$env" "$present" "$version"
  else
    printf '%s: host %s, extensión no instalada\n' "$env" "$present"
  fi
}

# Selección de hosts. Interactiva en una tty (marcando qué hosts están
# presentes y en cuáles ya hay extensión); sin tty exige --env o --all.
select_agents() {
  local requested_all=0
  local -a requested_envs=()
  while (($# > 0)); do
    case "$1" in
      --all)
        requested_all=1
        shift
        ;;
      --env)
        (($# >= 2)) || die "--env necesita un valor"
        agent_env_dir "$2" >/dev/null ||
          die "agente desconocido: '$2' (soportados: $(agent_envs | tr '\n' ' '))"
        requested_envs+=("$2")
        shift 2
        ;;
      *)
        error "argumento desconocido: $1"
        usage >&2
        exit 2
        ;;
    esac
  done

  if ((requested_all == 1)) || ((${#requested_envs[@]} > 0)); then
    if ((requested_all == 1)); then
      agent_envs
    else
      printf '%s\n' "${requested_envs[@]}"
    fi
    return 0
  fi

  if [[ ! -t 0 ]]; then
    error "sin terminal no se pregunta en qué agentes instalar:"
    error "pasá --env pi o --all."
    return 1
  fi

  agent_env_status pi >&2
  agent_env_status gentle-shell >&2
  printf 'Agentes a instalar/actualizar (números separados por espacio,' >&2
  printf ' «all», o Enter para omitir): ' >&2
  local answer=""
  read -r answer || answer=""
  [[ -n "$answer" ]] || return 0
  if [[ "$answer" == "all" || "$answer" == "ALL" ]]; then
    agent_envs
    return 0
  fi
  local env total pick
  total="$(agent_envs | wc -l)"
  for pick in $answer; do
    if [[ ! "$pick" =~ ^[0-9]+$ ]] || ((pick < 1 || pick > total)); then
      error "selección inválida: '$pick' (1..$total)"
      return 1
    fi
    env="$(agent_envs | sed -n "${pick}p")"
    printf '%s\n' "$env"
  done
}

cmd_agent() {
  local -a agents
  agents="$(select_agents "$@")" || exit $?
  local installed_any=0 env
  for env in $agents; do
    agent_env_present "$env" || {
      printf '%s: el host no está en esta máquina (%s); se omite.\n' \
        "$env" "$(agent_env_dir "$env")"
      continue
    }
    install_extension "$env"
    installed_any=1
  done
  if ((installed_any == 0)); then
    printf 'No se instaló la extensión en ningún agente.\n'
  fi
}

# --- update -----------------------------------------------------------------

cmd_update() {
  local previous latest
  previous="$(installed_version)"
  if [[ -z "$previous" ]]; then
    printf 'graphium no está instalado en %s; corré ./install.sh primero.\n' "$GRAPHIUM_BIN"
    return 0
  fi
  if ((IN_REPO == 1)); then
    printf 'Desde el repositorio el binario se compila del árbol de trabajo:\n'
    printf 'para actualizar corré ./install.sh (recompila e instala).\n'
    return 0
  fi
  latest="$(latest_release_version 2>/dev/null || true)"
  if [[ -z "$latest" ]]; then
    printf 'Todavía no hay releases publicados; el primero aparece al etiquetar\n'
    printf '(por ejemplo v0.1.0). Lo instalado (%s) queda como está.\n' "$previous"
    return 0
  fi
  if ! version_is_newer "$previous" "$latest"; then
    printf 'Ya está la última versión (%s).\n' "$previous"
    return 0
  fi
  printf 'Hay versión nueva: %s -> %s.\n' "$previous" "$latest"
  install_binary_from_release "" || return 1
  # La extensión se refresca en cada agente que ya la tiene: instalarla en
  # hosts nuevos es trabajo de `agent`, no de `update`.
  local env dest version_ext
  for env in $(agent_envs); do
    dest="$(agent_env_dir "$env")/graphium"
    version_ext="$(extension_installed_version "$dest")"
    if [[ -n "$version_ext" ]] && agent_env_present "$env"; then
      install_extension "$env" || printf 'No se pudo actualizar la extensión en %s.\n' "$env"
    fi
  done
  printf 'Actualizado a %s.\n' "$(installed_version)"
}

# --- uninstall --------------------------------------------------------------

cmd_uninstall() {
  local removed=0 env dest
  if [[ -f "$GRAPHIUM_BIN" || -L "$GRAPHIUM_BIN" ]]; then
    rm -f -- "$GRAPHIUM_BIN"
    printf 'Binario removido: %s\n' "$GRAPHIUM_BIN"
    removed=1
  else
    printf 'No hay binario en %s.\n' "$GRAPHIUM_BIN"
  fi
  for env in $(agent_envs); do
    dest="$(agent_env_dir "$env")/graphium"
    if [[ -d "$dest" ]]; then
      rm -rf -- "$dest"
      printf 'Extensión removida de %s\n' "$dest"
      removed=1
    fi
  done
  if [[ -d "$HOME/.graphium" ]]; then
    printf 'Los datos de exportes quedan en %s (borralos a mano si querés).\n' "$HOME/.graphium"
  fi
  ((removed == 1)) || printf 'Graphium no estaba instalado.\n'
}

# --- punto de entrada -------------------------------------------------------

main() {
  local command="${1:-install}"
  case "$command" in
    install)
      shift || true
      local version="${GRAPHIUM_VERSION:-}"
      while (($# > 0)); do
        case "$1" in
          --version)
            (($# >= 2)) || die "--version necesita un valor"
            version="$2"
            shift 2
            ;;
          *)
            error "argumento desconocido: $1"
            usage >&2
            exit 2
            ;;
        esac
      done
      install_binary 1
      warn_runtime_deps
      printf 'Siguiente paso: ./install.sh agent conecta la extensión de tu agente.\n'
      ;;
    agent)
      shift || true
      cmd_agent "$@"
      ;;
    update)
      shift || true
      (($# == 0)) || die "update no toma argumentos"
      cmd_update
      ;;
    uninstall)
      shift || true
      (($# == 0)) || die "uninstall no toma argumentos"
      cmd_uninstall
      ;;
    -h | --help | help)
      usage
      ;;
    *)
      error "comando desconocido: $command"
      usage >&2
      exit 2
      ;;
  esac
}

main "$@"
