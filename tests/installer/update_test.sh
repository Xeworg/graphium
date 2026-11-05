#!/usr/bin/env bash
#
# update: sin binario instalado avisa y sale bien; en modo repositorio
# deriva a ./install.sh; con un binario falso de la versión actual del árbol
# no inventa una actualización.

set -euo pipefail
# shellcheck source=lib.sh
source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

new_scratch
trap cleanup_scratch EXIT

# Sin binario: mensaje claro, exit 0 (no es un error del instalador).
out="$(run_installer update)"
assert_contains "$out" "no está instalado"

# En modo repositorio, update deriva a ./install.sh en vez de bajar nada.
mkdir -p "$SCRATCH_HOME/.local/bin"
printf '#!/bin/sh\necho "graphium %s"\n' "$(tree_version)" >"$SCRATCH_HOME/.local/bin/graphium"
chmod +x "$SCRATCH_HOME/.local/bin/graphium"
out="$(run_installer update)"
assert_contains "$out" "árbol de trabajo"
assert_contains "$out" "./install.sh"

# El binario falso sigue intacto: update en modo repo no toca nada.
version="$("$SCRATCH_HOME/.local/bin/graphium" --version)"
assert_contains "$version" "$(tree_version)"
