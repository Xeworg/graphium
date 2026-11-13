#!/usr/bin/env bash
#
# Dos hosts del registro pueden resolver al mismo directorio: `pi` sigue a
# PI_CODING_AGENT_DIR cuando está definida, y una sesión de gentle-shell la
# apunta al agente de gentle-shell. En ese caso el instalador tiene que
# instalar una vez y decirlo, no instalar dos veces el mismo destino mientras
# el resumen dice que fueron dos hosts.

set -euo pipefail
# shellcheck source=lib.sh
source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

new_scratch
trap cleanup_scratch EXIT

# El directorio de agente de gentle-shell por defecto, bajo el HOME de scratch.
shared="$SCRATCH_HOME/.gentle-shell/agent"
mkdir -p "$shared"

# PI_CODING_AGENT_DIR apunta al mismo lugar: `pi` y `gentle-shell` colisionan.
out="$(run_installer_with_env "PI_CODING_AGENT_DIR=$shared" agent --env pi --env gentle-shell)"

assert_contains "$out" "Agentes elegidos: pi gentle-shell"
assert_file "$shared/extensions/graphium/index.ts"

# Una sola copia: el segundo host avisa que el destino ya se instaló.
install_lines="$(printf '%s\n' "$out" | grep -c 'Extensión instalada' || true)"
if [[ "$install_lines" != "1" ]]; then
  printf 'FAIL: se instaló %s veces el mismo destino\n' "$install_lines"
  printf '%s\n' "$out"
  exit 1
fi
assert_contains "$out" "se instala una vez"
