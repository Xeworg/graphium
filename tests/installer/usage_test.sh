#!/usr/bin/env bash
#
# Errores de uso: --help lista los comandos; comando desconocido y
# argumento desconocido salen con código 2 y la ayuda por stderr.

set -euo pipefail
# shellcheck source=lib.sh
source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

new_scratch
trap cleanup_scratch EXIT

help="$(run_installer --help)"
for needle in "install.sh agent" "install.sh update" "install.sh uninstall" "PI_CODING_AGENT_DIR"; do
  assert_contains "$help" "$needle"
done

# El agente desconocido nombra los soportados.
if run_installer agent --env noexiste >/dev/null 2>&1; then
  printf 'FAIL: unknown agent env must fail\n'
  exit 1
fi
err="$(run_installer agent --env noexiste 2>&1 >/dev/null || true)"
assert_contains "$err" "pi"

if run_installer inventado >/dev/null 2>&1; then
  printf 'FAIL: unknown command must fail\n'
  exit 1
fi
if run_installer install --inventado >/dev/null 2>&1; then
  printf 'FAIL: unknown option must fail\n'
  exit 1
fi
