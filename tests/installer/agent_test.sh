#!/usr/bin/env bash
#
# agent instala la extensión en el host pi presente y reporta el estado;
# con el host ausente se omite con exit 0; sin tty y sin --env/--all falla
# pidiendo la elección explícita.

set -euo pipefail
# shellcheck source=lib.sh
source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

new_scratch
trap cleanup_scratch EXIT

# Sin host, agent --env pi se omite sin fallar: no hay nada que instalar.
out="$(run_installer agent --env pi)"
assert_contains "$out" "se omite"
assert_no_file "$SCRATCH_HOME/.pi/agent/extensions/graphium"

# Con el host presente, la extensión aterriza completa y con la versión del
# árbol.
mkdir -p "$SCRATCH_HOME/.pi/agent"
run_installer agent --env pi >/dev/null
for file in index.ts package.json README.md; do
  assert_file "$SCRATCH_HOME/.pi/agent/extensions/graphium/$file"
done
grep -q '"version"' "$SCRATCH_HOME/.pi/agent/extensions/graphium/package.json"

# --all sobre el mismo host reinstala (sobreescribe) sin fallar.
run_installer agent --all >/dev/null
assert_file "$SCRATCH_HOME/.pi/agent/extensions/graphium/index.ts"

# Sin tty y sin --env/--all: error accionable, no una pregunta colgada.
if run_installer agent >/dev/null 2>&1; then
  printf 'FAIL: agent without --env on a non-tty must fail\n'
  exit 1
fi
