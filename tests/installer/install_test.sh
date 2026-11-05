#!/usr/bin/env bash
#
# install compila (o reusa la build del runner), copia el binario a
# $HOME/.local/bin y el binario responde --version. Reinstalar es idempotente.

set -euo pipefail
# shellcheck source=lib.sh
source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

new_scratch
trap cleanup_scratch EXIT

run_installer install >/dev/null
assert_file "$SCRATCH_HOME/.local/bin/graphium"
version="$("$SCRATCH_HOME/.local/bin/graphium" --version)"
assert_contains "$version" "$(tree_version)"

# Reinstalar sobre un binario existente no falla y sigue siendo el mismo
# comando: correrlo de nuevo sobreescribe.
run_installer >/dev/null
assert_file "$SCRATCH_HOME/.local/bin/graphium"
"$SCRATCH_HOME/.local/bin/graphium" --version >/dev/null
