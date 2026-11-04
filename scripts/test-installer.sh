#!/usr/bin/env bash
#
# Corre la suite de tests del instalador (tests/installer/).
#
#   scripts/test-installer.sh
#
# Estos tests no son tests de cargo: ejercitan install.sh (install, agent,
# update, uninstall, errores de uso) contra un HOME de scratch creado con
# mktemp y un CARGO_TARGET_DIR compartido bajo el gitignored
# tmp/installer-target. El HOME real del usuario nunca se toca.
#
# Requisitos: bash, coreutils, rustc + cargo (los tests de instalación
# necesitan una build release). El runner compila el release una vez; cada
# test de install re-corre el mismo `cargo build --release` como chequeo de
# frescura rápido.

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"

usage() {
  printf '%s\n' \
    'Usage: scripts/test-installer.sh' \
    '' \
    'Runs every tests/installer/*_test.sh against a scratch HOME.' \
    'A release build is written to tmp/installer-target (override with' \
    'CARGO_TARGET_DIR). Exit status is non-zero when any test fails.'
}

for arg in "$@"; do
  case "$arg" in
    -h | --help)
      usage
      exit 0
      ;;
    *)
      printf 'test-installer.sh: unknown argument: %s\n' "$arg" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if ! command -v cargo >/dev/null 2>&1 || ! command -v rustc >/dev/null 2>&1; then
  printf 'test-installer.sh: cargo/rustc were not found on PATH.\n' >&2
  printf 'Install Rust with rustup (https://rustup.rs) and re-run.\n' >&2
  exit 1
fi

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$REPO_ROOT/tmp/installer-target}"
mkdir -p -- "$CARGO_TARGET_DIR"

printf 'Building the release profile once for the suite...\n'
(cd -- "$REPO_ROOT" && cargo build --release)

failures=0
for test in "$REPO_ROOT"/tests/installer/*_test.sh; do
  [[ -f "$test" ]] || {
    printf 'test-installer.sh: no tests found in tests/installer/\n' >&2
    exit 1
  }
  name="$(basename -- "$test")"
  printf '\n== %s ==\n' "$name"
  if bash -- "$test"; then
    printf 'PASS %s\n' "$name"
  else
    printf 'FAIL %s\n' "$name"
    failures=$((failures + 1))
  fi
done

if ((failures != 0)); then
  printf '\n%d test(s) failed.\n' "$failures" >&2
  exit 1
fi
printf '\nAll installer tests passed.\n'
