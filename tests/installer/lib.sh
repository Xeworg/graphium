#!/usr/bin/env bash
#
# Helpers compartidos por los tests del instalador. Cada test corre contra
# su propio HOME de scratch; el repositorio se resuelve desde tests/.

set -euo pipefail

TESTS_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# tests/ está un nivel abajo de la raíz: tests/installer/ está dos.
REPO_ROOT="$(cd -- "$TESTS_DIR/../.." && pwd)"
INSTALLER="$REPO_ROOT/install.sh"

# Versión del árbol de trabajo: la lee de Cargo.toml para que un bump de
# release no deje los tests del instalador desactualizados.
tree_version() {
  awk '
    /^\[workspace\.package\]$/ { in_ws = 1; next }
    /^\[/ { in_ws = 0 }
    in_ws && /^version *= *"/ {
      sub(/^version *= *"/, ""); sub(/".*$/, ""); print; exit
    }
  ' "$REPO_ROOT/Cargo.toml"
}

# Crea un HOME de scratch; el llamador lo borra con cleanup_scratch.
SCRATCH_HOME=""
new_scratch() {
  SCRATCH_HOME="$(mktemp -d)"
  printf '%s\n' "$SCRATCH_HOME"
}

cleanup_scratch() {
  [[ -n "$SCRATCH_HOME" ]] && rm -rf -- "$SCRATCH_HOME"
  SCRATCH_HOME=""
}

# Sin terminal de control no se puede simular «no hay terminal»: setsid(1) de
# util-linux es el que arma una sesión nueva y deja el proceso sin /dev/tty.
# Sin setsid los casos «sin terminal» no probarían nada (el instalador abriría
# la terminal que el runner heredó) y podrían colgarse esperando una tecla, así
# que el test falla en vez de pasar en falso.
require_no_terminal() {
  command -v setsid >/dev/null 2>&1 || {
    printf 'FAIL: setsid(1) no está en el PATH (util-linux): los tests necesitan\n' >&2
    printf 'correr el instalador sin terminal de control. Instalá util-linux.\n' >&2
    exit 1
  }
}

# Corre el instalador como lo vería un usuario con el HOME del scratch y sin
# heredar los hosts de agente de la máquina del desarrollador.
#
# El proceso va en una sesión nueva (setsid): no hay terminal de control, igual
# que en cron, CI o `docker run` sin -t. Stdin no alcanza para decirlo, porque
# con `curl -fsSL …/install.sh | bash` stdin es el script y aun así el humano
# está frente a una terminal: el instalador abre /dev/tty a propósito y el test
# se quedaría esperando una tecla.
#
# CARGO_HOME se fija al real: con el HOME de scratch cargo usaría una caché
# de crates vacía, volvería a bajar las dependencias y recompilaría todo el
# árbol release en cada test. Fijarlo reusa la caché y el
# CARGO_TARGET_DIR del runner, así que los tests no dependen de la red.
REAL_CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
run_installer() {
  require_no_terminal
  env -u PI_CODING_AGENT_DIR -u GENTLE_SHELL_AGENT_DIR \
    HOME="$SCRATCH_HOME" CARGO_HOME="$REAL_CARGO_HOME" \
    setsid -w -- bash -- "$INSTALLER" "$@" </dev/null
}

# Igual que run_installer pero con una asignación de entorno extra, para
# probar los overrides del registro de hosts (PI_CODING_AGENT_DIR y los que
# vengan).
run_installer_with_env() {
  local assignment="$1"
  shift
  require_no_terminal
  env -u PI_CODING_AGENT_DIR -u GENTLE_SHELL_AGENT_DIR \
    HOME="$SCRATCH_HOME" CARGO_HOME="$REAL_CARGO_HOME" \
    "$assignment" setsid -w -- bash -- "$INSTALLER" "$@" </dev/null
}

# El caso de un script (`curl | bash` en cron, CI o `docker run` sin -t): el
# script llega por stdin y no hay terminal de control. Tiene que terminar con
# el mensaje accionable, nunca esperar una tecla.
run_piped_no_terminal() {
  require_no_terminal
  env -u PI_CODING_AGENT_DIR -u GENTLE_SHELL_AGENT_DIR \
    HOME="$SCRATCH_HOME" CARGO_HOME="$REAL_CARGO_HOME" \
    setsid -w -- bash -s -- "$@" <"$INSTALLER"
}

# Entradas del registro de hosts tal como las publica la ayuda del instalador:
# una por línea, como <nombre>|<variable de entorno>|<dir por defecto>.
registry_entries() {
  run_installer --help |
    sed -n 's/^  \([a-z][a-z-]*\)  *\([A-Z][A-Z0-9_]*\) o ~\/\(.*\)$/\1|\2|\3/p'
}

# Nombres de agente que la ayuda publica en la línea de --env.
usage_env_agents() {
  run_installer --help |
    sed -n 's/^  --env <name>  Agente destino (repetible): \(.*\)\.$/\1/p' |
    tr ',' ' '
}

# ¿Hay con qué armar un pseudo-terminal en esta máquina? El `script` de
# util-linux es el que entiende `-q -e -c`; con otro (BSD) los casos
# interactivos se saltean.
pty_available() {
  command -v script >/dev/null 2>&1 &&
    command -v timeout >/dev/null 2>&1 &&
    script --version 2>/dev/null | grep -q 'util-linux'
}

# Tope de un caso interactivo. Un `install` desde el repositorio compila el
# plano y el árbol release antes de llegar al checklist: en una máquina
# tranquila son ~60 s, y con el tope viejo (60 s) el caso moría a mitad de
# compilación. Se puede subir con GRAPHIUM_TEST_TIMEOUT.
PTY_TIMEOUT="${GRAPHIUM_TEST_TIMEOUT:-300}"

# Corre una línea de comando bajo un pseudo-terminal (script de util-linux) y
# devuelve su salida combinada y el código de salida del comando.
#
# Un caso que se pasa del tope se reporta con el motivo antes de devolver el
# 124 de timeout: morirse en silencio deja un test que parece colgado y no dice
# ni qué caso fue ni cuánto esperó.
_pty_run() {
  local term="$1" input="$2" command_line="$3" status=0
  printf '%s' "$input" |
    TERM="$term" timeout -k 5 "$PTY_TIMEOUT" \
      script -qec "stty rows 40 cols 100 2>/dev/null; $command_line" /dev/null || status=$?
  if ((status == 124 || status == 137)); then
    printf 'FAIL: el caso interactivo no terminó en %ss (TERM=%s):\n  %s\n' \
      "$PTY_TIMEOUT" "$term" "$command_line" >&2
    printf 'Si el caso hace un `install` desde el repositorio, subí el tope con\n' >&2
    printf 'GRAPHIUM_TEST_TIMEOUT=<segundos>.\n' >&2
  fi
  return "$status"
}

# run_installer_pty <term> <input> [args...]
#
# Corre el instalador bajo un pseudo-terminal para ejercitar los caminos
# interactivos sin una terminal humana: el selector ve una tty real y TERM como
# el de un usuario. <input> es lo que se teclea.
run_installer_pty() {
  local term="$1" input="$2"
  shift 2
  local -a cmd=(
    env -u PI_CODING_AGENT_DIR -u GENTLE_SHELL_AGENT_DIR
    "HOME=$SCRATCH_HOME" "CARGO_HOME=$REAL_CARGO_HOME" bash -- "$INSTALLER"
  )
  cmd+=("$@")
  _pty_run "$term" "$input" "$(printf '%q ' "${cmd[@]}")"
}

# run_installer_piped_pty <term> <input> [args...]
#
# El caso que el dueño ejecuta: `curl -fsSL …/install.sh | bash` en una
# terminal. Stdin es el script (un archivo, acá) y la terminal de control es la
# del pty, así que el checklist solo puede aparecer si el instalador abre
# /dev/tty.
run_installer_piped_pty() {
  local term="$1" input="$2"
  shift 2
  local -a cmd=(
    env -u PI_CODING_AGENT_DIR -u GENTLE_SHELL_AGENT_DIR
    "HOME=$SCRATCH_HOME" "CARGO_HOME=$REAL_CARGO_HOME" bash -s --
  )
  cmd+=("$@")
  _pty_run "$term" "$input" \
    "$(printf '%q ' "${cmd[@]}")< $(printf '%q' "$INSTALLER")"
}

# run_installer_closed_stdin_pty <term> [args...]
#
# Un run con stdin cerrado (`0<&-`) dentro de una terminal: no es interactivo y
# tiene que contestar el mensaje accionable en vez de abrir /dev/tty y esperar
# una tecla que no va a llegar.
run_installer_closed_stdin_pty() {
  local term="$1"
  shift
  local -a cmd=(
    env -u PI_CODING_AGENT_DIR -u GENTLE_SHELL_AGENT_DIR
    "HOME=$SCRATCH_HOME" "CARGO_HOME=$REAL_CARGO_HOME" bash -- "$INSTALLER"
  )
  cmd+=("$@")
  _pty_run "$term" "" "$(printf '%q ' "${cmd[@]}") 0<&-"
}

# Plataforma de los assets, igual que las publica el instalador.
mock_platform() {
  case "$(uname -m)" in
    x86_64 | amd64) printf 'linux-x86_64' ;;
    aarch64 | arm64) printf 'linux-aarch64' ;;
    *) return 1 ;;
  esac
}

# make_mock_release <version>
#
# Un «servidor» de releases con los mismos nombres de asset, servido por
# `file://`: el instalador no sabe la diferencia, así que un test ejercita el
# camino publicado sin red y sin publicar nada. Deja la versión en
# $SCRATCH_HOME/mock-release y la ruta en MOCK_RELEASES; el llamador pasa
# GRAPHIUM_RELEASES_URL="file://$MOCK_RELEASES" y GRAPHIUM_VERSION=<version>.
# Llamarlo varias veces con versiones distintas deja todas en el mismo
# servidor.
MOCK_RELEASES=""
make_mock_release() {
  local version="$1" platform stage
  platform="$(mock_platform)" || {
    printf 'FAIL: no hay assets publicados para %s\n' "$(uname -m)" >&2
    exit 1
  }
  MOCK_RELEASES="$SCRATCH_HOME/mock-release"
  stage="$MOCK_RELEASES/stage-$version"
  mkdir -p "$MOCK_RELEASES/download/v$version" "$stage/bin" "$stage/ext/graphium"
  printf '#!/bin/sh\necho "graphium %s"\n' "$version" >"$stage/bin/graphium"
  chmod +x "$stage/bin/graphium"
  tar -czf "$MOCK_RELEASES/download/v$version/graphium-$platform.tar.gz" \
    -C "$stage/bin" graphium
  printf 'export default function graphium() { /* %s */ }\n' "$version" \
    >"$stage/ext/graphium/index.ts"
  printf '{\n  "name": "@xeworg/graphium-pi",\n  "version": "%s"\n}\n' "$version" \
    >"$stage/ext/graphium/package.json"
  tar -czf "$MOCK_RELEASES/download/v$version/graphium-pi-extension.tar.gz" \
    -C "$stage/ext" graphium
  (
    cd "$MOCK_RELEASES/download/v$version"
    sha256sum -- "graphium-$platform.tar.gz" graphium-pi-extension.tar.gz >SHA256SUMS
  )
  printf '%s\n' "$MOCK_RELEASES"
}

# La versión que declara una extensión instalada (su package.json).
extension_version() {
  [[ -f "$1/package.json" ]] || return 0
  sed -n 's/.*"version"[: ]*"\([^"]*\)".*/\1/p' "$1/package.json" | head -1
}

assert_file() {
  [[ -f "$1" ]] || {
    printf 'assert_file FAILED: %s does not exist\n' "$1"
    return 1
  }
}

assert_no_file() {
  [[ ! -e "$1" ]] || {
    printf 'assert_no_file FAILED: %s exists\n' "$1"
    return 1
  }
}

assert_contains() {
  local haystack="$1" needle="$2"
  printf '%s\n' "$haystack" | grep -qF -- "$needle" || {
    printf 'assert_contains FAILED: %s not found in:\n%s\n' "$needle" "$haystack"
    return 1
  }
}

assert_not_contains() {
  local haystack="$1" needle="$2"
  if printf '%s\n' "$haystack" | grep -qF -- "$needle"; then
    printf 'assert_not_contains FAILED: %s found in:\n%s\n' "$needle" "$haystack"
    return 1
  fi
}
