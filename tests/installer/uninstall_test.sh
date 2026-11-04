#!/usr/bin/env bash
#
# uninstall saca el binario y la extensión instalada, conserva ~/.graphium,
# y correrlo de nuevo no falla (Graphium no estaba instalado).

set -euo pipefail
# shellcheck source=lib.sh
source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

new_scratch
trap cleanup_scratch EXIT

# Ciclo completo: install + agent + uninstall.
mkdir -p "$SCRATCH_HOME/.pi/agent" "$SCRATCH_HOME/.graphium"
printf 'x' >"$SCRATCH_HOME/.graphium/exports-marker"
run_installer install >/dev/null
run_installer agent --env pi >/dev/null
assert_file "$SCRATCH_HOME/.local/bin/graphium"
assert_file "$SCRATCH_HOME/.pi/agent/extensions/graphium/index.ts"

run_installer uninstall >/dev/null
assert_no_file "$SCRATCH_HOME/.local/bin/graphium"
assert_no_file "$SCRATCH_HOME/.pi/agent/extensions/graphium"
assert_file "$SCRATCH_HOME/.graphium/exports-marker"

# Segunda pasada: nada que borrar, exit 0 igual.
out="$(run_installer uninstall)"
assert_contains "$out" "no estaba instalado"
