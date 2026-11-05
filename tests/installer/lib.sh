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
# heredar el host de Pi de la máquina del desarrollador.
run_installer() {
  env -u PI_CODING_AGENT_DIR HOME="$SCRATCH_HOME" bash -- "$INSTALLER" "$@"
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
