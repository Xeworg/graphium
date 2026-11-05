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

# Corre el instalador como lo vería un usuario con el HOME del scratch y sin
# heredar los hosts de agente de la máquina del desarrollador. Stdin es
# /dev/null: ningún test queda esperando una tecla aunque el runner corra en
# una terminal (el selector interactivo solo aparece con una tty real).
#
# CARGO_HOME se fija al real: con el HOME de scratch cargo usaría una caché
# de crates vacía, volvería a bajar las dependencias y recompilaría todo el
# árbol release en cada test. Fijarlo reusa la caché y el
# CARGO_TARGET_DIR del runner, así que los tests no dependen de la red.
REAL_CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
run_installer() {
  env -u PI_CODING_AGENT_DIR -u GENTLE_SHELL_AGENT_DIR \
    HOME="$SCRATCH_HOME" CARGO_HOME="$REAL_CARGO_HOME" \
    bash -- "$INSTALLER" "$@" </dev/null
}

# Igual que run_installer pero con una asignación de entorno extra, para
# probar los overrides del registro de hosts (PI_CODING_AGENT_DIR y los que
# vengan).
run_installer_with_env() {
  local assignment="$1"
  shift
  env -u PI_CODING_AGENT_DIR -u GENTLE_SHELL_AGENT_DIR \
    HOME="$SCRATCH_HOME" CARGO_HOME="$REAL_CARGO_HOME" \
    "$assignment" bash -- "$INSTALLER" "$@" </dev/null
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

# run_installer_pty <term> <input> [args...]
#
# Corre el instalador bajo un pseudo-terminal (script de util-linux) para
# ejercitar los caminos interactivos sin una terminal humana: el selector ve
# una tty real y TERM como el de un usuario. <input> es lo que se teclea.
# Devuelve la salida combinada y el código de salida del instalador.
run_installer_pty() {
  local term="$1" input="$2"
  shift 2
  local -a cmd=(
    env -u PI_CODING_AGENT_DIR -u GENTLE_SHELL_AGENT_DIR
    "HOME=$SCRATCH_HOME" "CARGO_HOME=$REAL_CARGO_HOME" bash -- "$INSTALLER"
  )
  cmd+=("$@")
  printf '%s' "$input" |
    TERM="$term" timeout 60 script -qec "$(printf '%q ' "${cmd[@]}")" /dev/null
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
