#!/usr/bin/env bash
#
# Elección de agentes en una terminal:
# - sin terminal de control, agent sin --env/--all falla nombrando todos los
#   hosts del registro;
# - con TERM=dumb no hay checklist: se usa el prompt numérico, donde Enter
#   omite, un número instala solo ese host y un número inválido falla sin
#   instalar;
# - con una terminal normal el checklist arranca con los hosts detectados
#   marcados: flechas y espacio cambian el marcado, «a» marca/desmarca todo,
#   Enter instala y q o Esc cancelan sin instalar;
# - cada fila dice si el host está, qué versión tiene instalada y a cuál la
#   lleva esta corrida; los hosts ausentes se nombran como tales;
# - ./install.sh ofrece la elección al final y se puede saltear.

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
if [[ -z "$entries" || -z "$first_host" || -z "$first_dir" ]]; then
  printf 'FAIL: la ayuda no publica el registro de hosts\n'
  exit 1
fi

# 1) Sin tty y sin flags: error accionable que nombra el registro completo.
err="$(run_installer agent 2>&1 >/dev/null || true)"
for host in $hosts; do
  assert_contains "$err" "$host"
done
assert_contains "$err" "--all"

if ! pty_available; then
  printf 'SKIP: sin script/timeout no se puede armar un pty; corren los casos sin tty\n'
  exit 0
fi

# Hosts detectados: los directorios por defecto del registro.
while IFS='|' read -r _name _env_name default_dir; do
  mkdir -p "$SCRATCH_HOME/$default_dir"
done <<<"$entries"

# 2) TERM=dumb: el checklist no se usa y aparece el prompt numérico de
# siempre; Enter solo omite.
out="$(run_installer_pty dumb $'\n' agent)"
assert_contains "$out" "Agentes a instalar/actualizar"
assert_contains "$out" "No se instaló la extensión en ningún agente"
while IFS='|' read -r _name _env_name default_dir; do
  assert_no_file "$SCRATCH_HOME/$default_dir/extensions/graphium"
done <<<"$entries"

# 3) Un número fuera de rango falla sin instalar nada.
if run_installer_pty dumb $'9\n' agent >/dev/null 2>&1; then
  printf 'FAIL: una selección fuera de rango debe fallar\n'
  exit 1
fi
while IFS='|' read -r _name _env_name default_dir; do
  assert_no_file "$SCRATCH_HOME/$default_dir/extensions/graphium"
done <<<"$entries"

# 4) El número es la posición en el registro: 1 instala solo el primero.
out="$(run_installer_pty dumb $'1\n' agent)"
assert_contains "$out" "Agentes elegidos: $first_host"
assert_file "$SCRATCH_HOME/$first_dir/extensions/graphium/index.ts"
while IFS='|' read -r _name _env_name default_dir; do
  if [[ "$default_dir" == "$first_dir" ]]; then
    continue
  fi
  assert_no_file "$SCRATCH_HOME/$default_dir/extensions/graphium"
done <<<"$entries"

# 5) Checklist real (TERM normal): q y Esc cancelan sin instalar nada.
out="$(run_installer_pty xterm-256color 'q' agent)"
assert_contains "$out" "cancelada"
assert_contains "$out" "No se instaló la extensión en ningún agente"
out="$(run_installer_pty xterm-256color $'\033' agent)"
assert_contains "$out" "cancelada"

# 6) Flechas y espacio: ↓ baja el cursor, espacio desmarca ese host y Enter
# confirma, así que solo queda marcado el primero. Necesita un segundo host en
# el registro para tener a dónde bajar.
if [[ -n "$second_host" ]]; then
  out="$(run_installer_pty xterm-256color $'\033[B \n' agent)"
  assert_contains "$out" "Agentes elegidos: $first_host"
  assert_not_contains "$out" "Agentes elegidos: $first_host $second_host"
fi

# 7) «a» desmarca todo (arrancaban marcados) y Enter confirma sin instalar.
out="$(run_installer_pty xterm-256color $'a\n' agent)"
assert_contains "$out" "No se instaló la extensión en ningún agente"

# 8) Checklist real: Enter confirma los hosts detectados (todos marcados al
# arrancar) y los instala a todos.
out="$(run_installer_pty xterm-256color $'\n' agent)"
assert_contains "$out" "Agentes elegidos:"
while IFS='|' read -r _name _env_name default_dir; do
  assert_file "$SCRATCH_HOME/$default_dir/extensions/graphium/index.ts"
done <<<"$entries"

# 9) ./install.sh en una terminal ofrece la elección al final; q la saltea y
# deja el siguiente paso a mano.
out="$(run_installer_pty xterm-256color 'q' install)"
assert_contains "$out" "Siguiente paso: ./install.sh agent"
assert_file "$SCRATCH_HOME/.local/bin/graphium"

# 10) ./install.sh con una terminal tonta cae al prompt numérico: Enter saltea.
out="$(run_installer_pty dumb $'\n' install)"
assert_contains "$out" "Agentes a instalar/actualizar"
assert_contains "$out" "Siguiente paso: ./install.sh agent"

# 11) Cada fila del checklist dice si el host está en la máquina y qué versión
# tiene instalada: una extensión vieja se marca con su versión y con la que
# esta corrida va a dejar.
old_ext="$SCRATCH_HOME/$first_dir/extensions/graphium"
mkdir -p "$old_ext"
printf '{\n  "name": "@xeworg/graphium-pi",\n  "version": "0.0.1"\n}\n' \
  >"$old_ext/package.json"
# El segundo host ya no está: su fila tiene que decirlo, no callarse.
if [[ -n "$second_host" ]]; then
  rm -rf -- "$SCRATCH_HOME/$(printf '%s\n' "$entries" | sed -n '2s/^[^|]*|[^|]*|//p')"
fi
out="$(run_installer_pty xterm-256color 'q' agent)"
assert_contains "$out" "host presente, extensión v0.0.1"
assert_contains "$out" "se instala v$(tree_version)"
if [[ -n "$second_host" ]]; then
  assert_contains "$out" "host ausente en esta máquina"
fi
