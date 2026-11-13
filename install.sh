#!/usr/bin/env bash
#
# Instalador / desinstalador de Graphium (Linux).
#
#   ./install.sh [install]      Compila con cargo (release), instala
#                               $HOME/.local/bin/graphium y al final ofrece
#                               conectar la extensión en tus agentes.
#   ./install.sh agent          Instala o actualiza la extensión en los
#                               agentes elegidos con un checklist en una
#                               tty; --env/--all sin preguntas.
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
  local name env_name default_dir
  printf '%s\n' \
    'Uso: ./install.sh [install] [--version <semver>]' \
    '       ./install.sh agent [--env <name> ... | --all] [--version <semver>]' \
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
    'Versión: sin --version baja el último release publicado. Con' \
    '--version <semver> (o GRAPHIUM_VERSION en el entorno) baja ESE release,' \
    'para el binario y para la extensión: las dos cosas son el mismo' \
    'release, y mezclarlas deja un agente que no sabe con qué binario' \
    'habla.' \
    'GRAPHIUM_RELEASES_URL cambia la base de los releases; sirve para' \
    'ejercitar el camino publicado contra un servidor local.' \
    '' \
    'Comandos:' \
    '  install    Instala (el comando por omisión). Con un release más nuevo' \
    '             publicado, actualiza el binario instalado. En una terminal' \
    '             termina ofreciendo conectar la extensión en tus agentes,' \
    '             con opción de saltear; sin tty solo deja el siguiente paso.' \
    '  agent      Instala la extensión de Graphium (extensions/pi/graphium)' \
    '             en los agentes de IA elegidos. Detecta los hosts presentes' \
    '             (ver «Hosts de agente»), en cuáles ya está instalada y abre' \
    '             un checklist: ↑/↓ para moverte, espacio para marcar o' \
    '             desmarcar, «a» para todos, Enter para confirmar y q o Esc' \
    '             para salir sin instalar nada. Arranca con los hosts' \
    '             detectados ya marcados. Si la terminal no da para el' \
    '             checklist (TERM tonta, sin tput/terminfo) pregunta con' \
    '             números. Sin tty hay que pasar --env <name> (uno o más)' \
    '             o --all. Con --version instala la extensión de ese release' \
    '             en vez de la del último.' \
    '  update     Si hay un release más nuevo que lo instalado, actualiza el' \
    '             binario y refresca la extensión en cada agente que ya la' \
    '             tiene. Antes del primer release avisa y no hace nada.' \
    '  uninstall  Saca el binario y la extensión de los agentes que la' \
    '             tienen. Los datos de exportes (~/.graphium) se conservan.' \
    '' \
    'Opciones de agent:' \
    "  --env <name>  Agente destino (repetible): $(agent_envs_list)." \
    '  --all         Todos los hosts detectados.' \
    '  Con --env o --all no se pregunta: la selección es la que pediste.' \
    '' \
    'Hosts de agente (registro: nombre, variable de entorno, directorio):'
  while IFS='|' read -r name env_name default_dir; do
    [[ -n "$name" ]] || continue
    printf '  %-13s %s o ~/%s\n' "$name" "$env_name" "$default_dir"
  done <<<"$AGENT_REGISTRY"
  printf '%s\n' \
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
# `GRAPHIUM_RELEASES_URL` existe para poder ejercitar el camino publicado sin
# publicar: un servidor local con los mismos nombres de asset alcanza, y el
# script no sabe la diferencia. En uso normal no se define.
RELEASES_URL="${GRAPHIUM_RELEASES_URL:-https://github.com/Xeworg/graphium/releases}"
# La versión de release que se pidió, si se pidió: `--version` en `install`
# o en `agent`, o `GRAPHIUM_VERSION` en el entorno. Vacía significa «el
# último publicado». Es una sola variable porque el binario y la extensión
# son el mismo release: bajar uno de una versión y el otro de otra deja un
# agente que no sabe con qué binario habla.
RELEASE_VERSION="${GRAPHIUM_VERSION:-}"

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

# El plano es una app Svelte y el binario embebe su bundle: compilar desde el
# repositorio necesita Node y npm antes de cargo (frontend-consolidacion, L3).
require_node_toolchain() {
  local missing=0
  if ! command -v node >/dev/null 2>&1; then
    error "node no fue encontrado en el PATH."
    missing=1
  fi
  if ! command -v npm >/dev/null 2>&1; then
    error "npm no fue encontrado en el PATH."
    missing=1
  fi
  if ((missing != 0)); then
    error "Graphium compila el plano (Svelte + Vite) antes del binario: instalá"
    error "Node 22 o superior (https://nodejs.org, nvm o el gestor de tu distribución)"
    error "y volvé a correr ./install.sh. Alternativa sin compilar: el instalador de"
    error "un release publicado, que ya trae el bundle adentro del binario."
    exit 1
  fi
}

# `tauri::generate_context!()` embebe `crates/gx-app/frontend/dist` al compilar:
# el bundle de Vite tiene que existir cuando cargo empieza.
build_frontend_bundle() {
  printf 'Compilando el plano (npm ci + npm run build)...\n'
  (cd -- "$SCRIPT_DIR/crates/gx-app/frontend" &&
    npm ci --no-fund --no-audit &&
    npm run build) ||
    die "el build del plano falló: sin crates/gx-app/frontend/dist el binario no compila"
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
  require_node_toolchain
  build_frontend_bundle
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
#
# `requested` es la versión que pidió el humano (`--version` o
# `GRAPHIUM_VERSION`). Con versión se baja ese release exacto; sin versión se
# resuelve el último publicado. Antes se descartaba en silencio y el instalador
# bajaba `/latest` aunque el humano hubiera fijado una versión.
install_binary() {
  local announce_update="${1:-0}" requested="${2:-$RELEASE_VERSION}"
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
    latest="${requested:-$(latest_release_version 2>/dev/null || true)}"
    if [[ -n "$previous" && -n "$latest" ]] && version_is_newer "$previous" "$latest"; then
      printf 'Actualizando: %s -> %s\n' "$previous" "$latest"
    fi
    install_binary_from_release "$requested"
  fi
  if ((announce_update == 1)) && [[ -n "$previous" ]]; then
    printf 'La versión anterior era %s; ahora es %s.\n' "$previous" "$(installed_version)"
  fi
}

# --- la extensión de los agentes --------------------------------------------
#
# Registro único de hosts de agente. Cada línea es
#
#   <nombre>|<variable de entorno que manda>|<directorio por defecto bajo $HOME>
#
# y de acá salen la ayuda, la detección, el estado, el selector y la
# instalación: sumar un agente es agregar una línea (R1).
#
# gentle-shell es un fork de Pi instalado en otra ruta: son dos hosts
# distintos, cada uno con su copia de la extensión (no hay conflicto: cada
# host registra su propio MCP y sus sesiones son independientes).
AGENT_REGISTRY='pi|PI_CODING_AGENT_DIR|.pi/agent
gentle-shell|GENTLE_SHELL_AGENT_DIR|.gentle-shell/agent'

# Nombres del registro, uno por línea y en orden.
agent_envs() {
  local name _env_name _default_dir
  while IFS='|' read -r name _env_name _default_dir; do
    [[ -n "$name" ]] && printf '%s\n' "$name"
  done <<<"$AGENT_REGISTRY"
}

# Los nombres del registro separados por coma, para la ayuda y los errores.
agent_envs_list() {
  local out="" name
  while IFS= read -r name; do
    [[ -n "$name" ]] || continue
    out="${out:+$out, }$name"
  done <<<"$(agent_envs)"
  printf '%s\n' "$out"
}

# Directorio de extensiones de un host, o error si el nombre no está en el
# registro (así --env rechaza agentes desconocidos).
agent_env_dir() {
  local wanted="$1" name env_name default_dir value=""
  while IFS='|' read -r name env_name default_dir; do
    [[ "$name" == "$wanted" ]] || continue
    if [[ -n "$env_name" ]]; then
      # Indirección explícita: `${!var}` sobre una variable sin definir rompe
      # bajo `set -u` en bash viejos, y el nombre sale del registro, nunca de
      # la entrada del usuario.
      eval "value=\${$env_name-}"
    fi
    printf '%s\n' "${value:-$HOME/$default_dir}/extensions"
    return 0
  done <<<"$AGENT_REGISTRY"
  return 1
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
  download_verified "graphium-pi-extension.tar.gz" "$RELEASE_VERSION" || return 1
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

# Estado corto de un host para el checklist: presencia y versión instalada.
agent_env_detail() {
  local env="$1" version=""
  if ! agent_env_present "$env"; then
    printf 'host ausente en esta máquina'
    return 0
  fi
  version="$(extension_installed_version "$(agent_env_dir "$env")/graphium")"
  if [[ -n "$version" ]]; then
    printf 'host presente, extensión instalada v%s' "$version"
  else
    printf 'host presente, extensión no instalada'
  fi
}

# --- selector interactivo (checklist) ---------------------------------------

# Estado del checklist en globales: bash no tiene cierres y redibujar necesita
# ver el estado completo. Se resetea en cada llamada.
AGENT_TUI_NAMES=()
AGENT_TUI_SELECTED=()
AGENT_TUI_CURSOR=0
AGENT_TUI_DRAWN=0
AGENT_TUI_COLS=80
AGENT_TUI_STTY=""
AGENT_TUI_ACTIVE=0
AGENT_TUI_CANCELLED=0
AGENT_TUI_KEY=""
AGENT_TUI_HEADER="¿En qué agentes instalar la extensión de Graphium?"
AGENT_TUI_FOOTER="↑/↓ mover · espacio marcar · a todos · Enter confirmar · q salir"

# Un número de la terminal (columnas o líneas), con un valor razonable cuando
# el driver no lo sabe.
agent_tui_tput() {
  local value=""
  value="$(tput "$1" 2>/dev/null || true)"
  if [[ ! "$value" =~ ^[0-9]+$ ]] || ((value == 0)); then
    if [[ "$1" == "cols" ]]; then
      value=80
    else
      value=24
    fi
  fi
  printf '%s\n' "$value"
}

# ¿La terminal soporta el checklist? Sin tty en stdin y stderr, con TERM tonta
# o sin stty/tput/terminfo se usa el prompt numerico de siempre (R3).
agent_tui_capable() {
  [[ -t 0 && -t 2 ]] || return 1
  [[ -n "${TERM:-}" && "${TERM}" != "dumb" ]] || return 1
  command -v stty >/dev/null 2>&1 || return 1
  command -v tput >/dev/null 2>&1 || return 1
  tput cols >/dev/null 2>&1 || return 1
  return 0
}

# Modo crudo: sin eco y sin canónico, una tecla por lectura. No se toca isig:
# Ctrl-C sigue interrumpiendo y el trap restaura la terminal.
agent_tui_raw_on() {
  AGENT_TUI_STTY="$(stty -g)" || return 1
  if ! stty -echo -icanon min 1 time 0; then
    agent_tui_raw_off
    return 1
  fi
  AGENT_TUI_ACTIVE=1
  printf '\033[?25l' >&2
}

agent_tui_raw_off() {
  [[ -n "$AGENT_TUI_STTY" ]] || return 0
  stty "$AGENT_TUI_STTY" 2>/dev/null || true
  AGENT_TUI_STTY=""
}

# Restaura la terminal en cualquier salida: fin normal, q, Ctrl-C o señal.
agent_tui_cleanup() {
  ((AGENT_TUI_ACTIVE == 1)) || return 0
  agent_tui_raw_off
  printf '\033[?25h' >&2
  AGENT_TUI_ACTIVE=0
}

# Recorta una línea al ancho de la terminal: el bloque se redibuja contando
# líneas impresas, así que ninguna puede envolver.
agent_tui_fit() {
  local text="$1" cols="${AGENT_TUI_COLS:-80}"
  if ((${#text} >= cols)); then
    printf '%s' "${text:0:$((cols - 1))}"
  else
    printf '%s' "$text"
  fi
}

# Dibuja (o redibuja) el checklist en stderr: encabezado, una línea por host
# y pie. Siempre mide lo mismo, así que subir esa cantidad de líneas vuelve
# al inicio del bloque.
agent_tui_draw() {
  local i pointer mark line
  if ((AGENT_TUI_DRAWN > 0)); then
    printf '\033[%dA' "$AGENT_TUI_DRAWN" >&2
  fi
  printf '\r\033[J' >&2
  printf '%s\n' "$(agent_tui_fit "$AGENT_TUI_HEADER")" >&2
  for i in "${!AGENT_TUI_NAMES[@]}"; do
    if ((i == AGENT_TUI_CURSOR)); then pointer='>'; else pointer=' '; fi
    if [[ "${AGENT_TUI_SELECTED[i]}" == 1 ]]; then mark='[x]'; else mark='[ ]'; fi
    line="$(printf '%s %s %-14s %s' "$pointer" "$mark" \
      "${AGENT_TUI_NAMES[i]}" "$(agent_env_detail "${AGENT_TUI_NAMES[i]}")")"
    printf '%s\n' "$(agent_tui_fit "$line")" >&2
  done
  printf '%s\n' "$(agent_tui_fit "$AGENT_TUI_FOOTER")" >&2
  AGENT_TUI_DRAWN=$((${#AGENT_TUI_NAMES[@]} + 2))
}

# Borra el bloque del checklist y deja el cursor al inicio de su primera
# línea, donde el llamador escribe el resumen de la elección.
agent_tui_erase() {
  if ((AGENT_TUI_DRAWN > 0)); then
    printf '\033[%dA\r\033[J' "$AGENT_TUI_DRAWN" >&2
    AGENT_TUI_DRAWN=0
  fi
}

# Lee una tecla en modo crudo y la normaliza en AGENT_TUI_KEY: up, down,
# space, all, enter, quit u otro. Las flechas llegan como ESC [ A/B; un Esc
# solo (o una secuencia desconocida) cuenta como salir.
agent_tui_read_key() {
  local key="" rest=""
  AGENT_TUI_KEY="otro"
  IFS= read -rsn1 key || return 1
  case "$key" in
    '' | $'\r' | $'\n') AGENT_TUI_KEY=enter ;;
    ' ') AGENT_TUI_KEY=space ;;
    a | A) AGENT_TUI_KEY=all ;;
    q | Q) AGENT_TUI_KEY=quit ;;
    j) AGENT_TUI_KEY=down ;;
    k) AGENT_TUI_KEY=up ;;
    $'\033')
      IFS= read -rsn1 -t 0.05 rest || {
        AGENT_TUI_KEY=quit
        return 0
      }
      if [[ "$rest" == '[' ]]; then
        IFS= read -rsn1 -t 0.05 rest || {
          AGENT_TUI_KEY=quit
          return 0
        }
        case "$rest" in
          A) AGENT_TUI_KEY=up ;;
          B) AGENT_TUI_KEY=down ;;
          *) AGENT_TUI_KEY=otro ;;
        esac
        return 0
      fi
      AGENT_TUI_KEY=quit
      ;;
  esac
  return 0
}

# Checklist interactivo (R2): flechas para moverse, espacio marca/desmarca,
# «a» marca o desmarca todos, Enter confirma y q/Esc cancela. Deja el resultado
# en AGENT_TUI_SELECTED y, si se salió sin confirmar, AGENT_TUI_CANCELLED.
# Devuelve 1 cuando la terminal no permite dibujarlo, para caer al prompt.
agent_select_tui() {
  local name i total all cols rows needed
  AGENT_TUI_NAMES=()
  AGENT_TUI_SELECTED=()
  AGENT_TUI_CURSOR=0
  AGENT_TUI_DRAWN=0
  AGENT_TUI_CANCELLED=0
  while IFS= read -r name; do
    [[ -n "$name" ]] || continue
    AGENT_TUI_NAMES+=("$name")
    # Estado inicial: marcados los hosts detectados en esta máquina (R2).
    if agent_env_present "$name"; then
      AGENT_TUI_SELECTED+=(1)
    else
      AGENT_TUI_SELECTED+=(0)
    fi
  done <<<"$(agent_envs)"
  total=${#AGENT_TUI_NAMES[@]}
  ((total > 0)) || return 1

  cols="$(agent_tui_tput cols)"
  rows="$(agent_tui_tput lines)"
  needed=$((total + 4))
  ((cols >= 40 && rows >= needed)) || return 1
  AGENT_TUI_COLS="$cols"

  trap 'agent_tui_cleanup' EXIT
  trap 'agent_tui_cleanup; exit 130' INT TERM HUP
  if ! agent_tui_raw_on; then
    trap - EXIT INT TERM HUP
    return 1
  fi
  agent_tui_draw
  while :; do
    if ! agent_tui_read_key; then
      AGENT_TUI_CANCELLED=1
      break
    fi
    case "$AGENT_TUI_KEY" in
      up)
        AGENT_TUI_CURSOR=$(((AGENT_TUI_CURSOR + total - 1) % total))
        ;;
      down)
        AGENT_TUI_CURSOR=$(((AGENT_TUI_CURSOR + 1) % total))
        ;;
      space)
        if [[ "${AGENT_TUI_SELECTED[AGENT_TUI_CURSOR]}" == 1 ]]; then
          AGENT_TUI_SELECTED[AGENT_TUI_CURSOR]=0
        else
          AGENT_TUI_SELECTED[AGENT_TUI_CURSOR]=1
        fi
        ;;
      all)
        all=1
        for i in "${!AGENT_TUI_SELECTED[@]}"; do
          [[ "${AGENT_TUI_SELECTED[i]}" == 1 ]] || all=0
        done
        for i in "${!AGENT_TUI_SELECTED[@]}"; do
          AGENT_TUI_SELECTED[i]=$((1 - all))
        done
        ;;
      enter) break ;;
      quit)
        AGENT_TUI_CANCELLED=1
        break
        ;;
      *) continue ;;
    esac
    agent_tui_draw
  done
  agent_tui_cleanup
  trap - EXIT INT TERM HUP
  agent_tui_erase
  return 0
}

# Respaldo sin checklist (R3): el prompt numerico de siempre, alimentado por
# el registro. Enter omite, «all» elige todos, un número elige esa posición.
agent_select_prompt() {
  local env answer="" total pick
  while IFS= read -r env; do
    agent_env_status "$env" >&2
  done <<<"$(agent_envs)"
  printf 'Agentes a instalar/actualizar (números separados por espacio,' >&2
  printf ' «all», o Enter para omitir): ' >&2
  read -r answer || answer=""
  [[ -n "$answer" ]] || return 0
  if [[ "$answer" == "all" || "$answer" == "ALL" ]]; then
    agent_envs
    return 0
  fi
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

# Elige hosts de forma interactiva: checklist si la terminal lo soporta y el
# prompt numerico si no (R2/R3). Imprime un nombre por línea en stdout; el
# dibujo del checklist va a stderr para no ensuciar la captura del llamador.
agent_select_interactive() {
  local i
  if agent_tui_capable && agent_select_tui; then
    if ((AGENT_TUI_CANCELLED == 1)); then
      printf 'Selección cancelada: no se instaló nada.\n' >&2
      return 0
    fi
    for i in "${!AGENT_TUI_NAMES[@]}"; do
      if [[ "${AGENT_TUI_SELECTED[i]}" == 1 ]]; then
        printf '%s\n' "${AGENT_TUI_NAMES[i]}"
      fi
    done
    return 0
  fi
  agent_select_prompt
}

# Selección de hosts. Interactiva en una tty (checklist con marcado, o prompt
# numerico si la terminal no da para el checklist); sin tty exige --env o
# --all.
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
    error "pasá --env <name> ($(agent_envs_list)) o --all."
    return 1
  fi

  agent_select_interactive
}

# Instala la extensión en los hosts elegidos: reporta cada uno y saltea los que
# no están en esta máquina.
install_agents() {
  local installed_any=0 env dest
  # Dos hosts pueden resolver al mismo directorio: `pi` sigue a
  # PI_CODING_AGENT_DIR cuando está definida, y en una sesión de gentle-shell
  # esa variable apunta justo al agente de gentle-shell. Copiar dos veces al
  # mismo destino no rompe nada, pero decir «instalado» dos veces sí es una
  # mentira, así que el segundo host lo dice y se saltea.
  local -A done_dirs=()
  for env in "$@"; do
    agent_env_present "$env" || {
      printf '%s: el host no está en esta máquina (%s); se omite.\n' \
        "$env" "$(agent_env_dir "$env")"
      continue
    }
    dest="$(agent_env_dir "$env")"
    if [[ -n "${done_dirs[$dest]:-}" ]]; then
      printf '%s: mismo destino que %s (%s); se instala una vez.\n' \
        "$env" "${done_dirs[$dest]}" "$dest"
      continue
    fi
    done_dirs[$dest]="$env"
    install_extension "$env"
    installed_any=1
  done
  if ((installed_any == 0)); then
    printf 'No se instaló la extensión en ningún agente.\n'
  fi
}

cmd_agent() {
  local selected
  selected="$(select_agents "$@")" || exit $?
  install_selected_agents "$selected"
}

# Lo que imprimió la selección se muestra antes de instalar y después se
# instala host por host (R6). Lo comparten `agent` y el final de `install`.
install_selected_agents() {
  local selected="$1" env
  local -a envs=()
  while IFS= read -r env; do
    if [[ -n "$env" ]]; then
      envs+=("$env")
    fi
  done <<<"$selected"
  if ((${#envs[@]} == 0)); then
    printf 'No se instaló la extensión en ningún agente.\n'
    return 0
  fi
  printf 'Agentes elegidos: %s\n' "${envs[*]}"
  install_agents "${envs[@]}"
}

# `./install.sh` termina ofreciendo la elección de agentes (R5): el checklist
# en una tty (o el prompt numerico si la terminal no lo soporta), con una
# salida clara para saltear y conectar el agente después a mano. Sin tty deja
# el «Siguiente paso» de siempre, así los scripts no se cuelgan (R4).
offer_agent_install() {
  local selected next_step
  next_step='Siguiente paso: ./install.sh agent conecta la extensión de tu agente.'
  if [[ ! -t 0 ]]; then
    printf '%s\n' "$next_step"
    return 0
  fi
  printf '\n'
  if ! selected="$(select_agents)"; then
    printf '%s\n' "$next_step"
    return 0
  fi
  if [[ -z "$selected" ]]; then
    printf '%s\n' "$next_step"
    return 0
  fi
  install_selected_agents "$selected"
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
  # La versión anunciada es la que se baja: entre el chequeo y la descarga
  # `/latest` puede moverse, y actualizar a otra cosa que la anunciada es peor
  # que no actualizar.
  install_binary_from_release "$latest" || return 1
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
      while (($# > 0)); do
        case "$1" in
          --version)
            (($# >= 2)) || die "--version necesita un valor"
            RELEASE_VERSION="${2#v}"
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
      offer_agent_install
      ;;
    agent)
      shift || true
      local agent_args=()
      while (($# > 0)); do
        case "$1" in
          --version)
            (($# >= 2)) || die "--version necesita un valor"
            RELEASE_VERSION="${2#v}"
            shift 2
            ;;
          *)
            agent_args+=("$1")
            shift
            ;;
        esac
      done
      cmd_agent ${agent_args[@]+"${agent_args[@]}"}
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
