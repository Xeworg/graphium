#!/usr/bin/env bash
#
# Actualizar es volver a correr `install`.
#
# Antes de tocar nada el instalador dice qué hay instalado y qué lo reemplaza;
# con un release local (file://) eso se puede ejercitar sin red y sin compilar:
#
# - la primera corrida dice que no hay binario y deja la versión pedida;
# - la segunda dice qué versión reemplaza y por cuál, y el binario instalado
#   queda en la versión nueva;
# - la extensión de un host que ya la tenía se informa «actualizada» con las
#   dos versiones, no «instalada», y su package.json queda en la nueva.

set -euo pipefail
# shellcheck source=lib.sh
source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

new_scratch
trap cleanup_scratch EXIT

entries="$(registry_entries)"
hosts="$(usage_env_agents)"
first_host="$(printf '%s\n' $hosts | head -1)"
first_dir="$(printf '%s\n' "$entries" | sed -n '1s/^[^|]*|[^|]*|//p')"
second_host="$(printf '%s\n' $hosts | sed -n '2p')"
second_dir="$(printf '%s\n' "$entries" | sed -n '2s/^[^|]*|[^|]*|//p')"
if [[ -z "$first_host" || -z "$first_dir" || -z "$second_host" || -z "$second_dir" ]]; then
  printf 'FAIL: el registro necesita dos hosts para este test\n'
  exit 1
fi
# Solo el primer host está en la máquina: así la corrida que instala la
# extensión le habla a un host que ya la tenía y a ninguno nuevo.
mkdir -p "$SCRATCH_HOME/$first_dir"

# Dos releases del mock: la versión vieja y la nueva, en el mismo «servidor».
make_mock_release 0.1.5 >/dev/null
make_mock_release 0.2.0 >/dev/null

# El camino de un script: el script llega por stdin y no hay terminal.
piped_install() {
  GRAPHIUM_RELEASES_URL="file://$MOCK_RELEASES" GRAPHIUM_VERSION="$1" \
    run_piped_no_terminal install
}
piped_agent() {
  GRAPHIUM_RELEASES_URL="file://$MOCK_RELEASES" GRAPHIUM_VERSION="$1" \
    run_piped_no_terminal agent --env "$2"
}

# El camino del humano: `curl … | bash` dentro de una terminal, y Enter
# confirma el checklist.
piped_install_pty() {
  GRAPHIUM_RELEASES_URL="file://$MOCK_RELEASES" GRAPHIUM_VERSION="$1" \
    run_installer_piped_pty xterm-256color $'\n' install
}

ext="$SCRATCH_HOME/$first_dir/extensions/graphium"

# 1) Primera corrida: no hay binario y lo dice.
out="$(piped_install 0.1.5)"
assert_contains "$out" "no hay nada instalado"
assert_contains "$out" "v0.1.5 del release"
assert_contains "$("$SCRATCH_HOME/.local/bin/graphium" --version)" "0.1.5"

# El host ya tenía la extensión, en la versión vieja.
piped_agent 0.1.5 "$first_host" >/dev/null
assert_contains "$(extension_version "$ext")" "0.1.5"

# 2) Segunda corrida en una terminal: dice qué reemplaza y por cuál, deja el
# binario en la versión nueva y actualiza la extensión que ya estaba.
out="$(piped_install_pty 0.2.0)"
assert_contains "$out" "se reemplaza v0.1.5"
assert_contains "$out" "v0.2.0 del release"
assert_contains "$out" "La versión anterior era 0.1.5; ahora es 0.2.0."
assert_contains "$("$SCRATCH_HOME/.local/bin/graphium" --version)" "0.2.0"
assert_contains "$out" "Extensión actualizada en"
assert_contains "$out" "graphium, v0.1.5 -> v0.2.0"
assert_not_contains "$out" "Extensión instalada"
assert_contains "$(extension_version "$ext")" "0.2.0"

# 3) En el árbol de trabajo pasa lo mismo con `agent --env`: la primera vez
# instala, la segunda actualiza el host que ya la tenía.
mkdir -p "$SCRATCH_HOME/$second_dir"
other="$SCRATCH_HOME/$second_dir/extensions/graphium"
out="$(run_installer agent --env "$second_host")"
assert_contains "$out" "Extensión instalada en"
assert_not_contains "$out" "Extensión actualizada"
assert_contains "$(extension_version "$other")" "$(tree_version)"

printf '{\n  "name": "@xeworg/graphium-pi",\n  "version": "0.0.1"\n}\n' \
  >"$other/package.json"
out="$(run_installer agent --env "$second_host")"
assert_contains "$out" "Extensión actualizada en"
assert_contains "$out" "graphium, v0.0.1 -> v$(tree_version)"
assert_contains "$(extension_version "$other")" "$(tree_version)"
