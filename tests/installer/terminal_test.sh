#!/usr/bin/env bash
#
# La terminal de control es la evidencia de un humano, no stdin.
#
# Con `curl -fsSL …/install.sh | bash` stdin es el propio script: no es una
# tty, y sin embargo el humano está mirando una terminal. El instalador abre
# /dev/tty para el checklist y para el prompt numérico, y solo cuando no hay
# terminal de control (cron, CI, `docker run` sin -t) o stdin está cerrado deja
# el mensaje accionable de siempre:
#
# - script pipeado en una terminal: el checklist aparece y el prompt numérico
#   funciona cuando la terminal no da para dibujarlo;
# - sin terminal de control, `install` termina con el «Siguiente paso» y
#   `agent` falla pidiendo --env/--all en vez de esperar una tecla;
# - stdin cerrado dentro de una terminal tampoco es interactivo: el fd 0 se
#   mira al arrancar (bash ocupa el 0 libre con el pipe de una `$(...)`), y el
#   mensaje es el mismo.

set -euo pipefail
# shellcheck source=lib.sh
source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

new_scratch
trap cleanup_scratch EXIT

entries="$(registry_entries)"
hosts="$(usage_env_agents)"
first_host="$(printf '%s\n' $hosts | head -1)"
first_dir="$(printf '%s\n' "$entries" | sed -n '1s/^[^|]*|[^|]*|//p')"
if [[ -z "$entries" || -z "$first_host" || -z "$first_dir" ]]; then
  printf 'FAIL: la ayuda no publica el registro de hosts\n'
  exit 1
fi

# Los directorios por defecto del registro existen: son los hosts que el
# checklist ofrece y marca al arrancar.
while IFS='|' read -r _name _env_name default_dir; do
  mkdir -p "$SCRATCH_HOME/$default_dir"
done <<<"$entries"

# Un release local (file://) para el camino publicado: el script pipeado no
# tiene archivo propio y siempre es modo standalone. Con estas variables en el
# entorno, `install` y `agent` bajan del release del mock en vez de compilar o
# de ir a GitHub.
make_mock_release 0.2.0 >/dev/null
export GRAPHIUM_RELEASES_URL="file://$MOCK_RELEASES"
export GRAPHIUM_VERSION="0.2.0"

# 1) Sin terminal de control: el mensaje accionable, y termina. Es el caso de
# cron, CI y `docker run` sin -t, con el script llegando por stdin.
out="$(run_piped_no_terminal install 2>&1)"
assert_contains "$out" "Siguiente paso: ./install.sh agent"
assert_file "$SCRATCH_HOME/.local/bin/graphium"

# 2) Sin terminal de control, `agent` sin --env/--all no espera: nombra el
# registro completo y sale con error.
err="$(run_piped_no_terminal agent 2>&1 >/dev/null || true)"
assert_contains "$err" "sin terminal no se pregunta"
for host in $hosts; do
  assert_contains "$err" "$host"
done
assert_contains "$err" "--all"
while IFS='|' read -r _name _env_name default_dir; do
  assert_no_file "$SCRATCH_HOME/$default_dir/extensions/graphium"
done <<<"$entries"

if ! pty_available; then
  printf 'SKIP: sin script/timeout de util-linux no se puede armar un pty; corren los casos sin terminal\n'
  exit 0
fi

# 3) Stdin cerrado dentro de una terminal: no hay quien teclee, así que el
# mensaje accionable en vez de abrir /dev/tty y quedarse esperando. El pty
# mezcla stdout y stderr, así que se capturan los dos.
err="$(run_installer_closed_stdin_pty xterm-256color agent 2>&1 || true)"
assert_contains "$err" "sin terminal no se pregunta"
assert_contains "$err" "--all"

# 4) El caso del dueño: el script llega por stdin (`curl … | bash`) y hay una
# terminal. `install` baja el release, y al final el checklist tiene que
# aparecer y aceptar el Enter que confirma los hosts detectados.
out="$(run_installer_piped_pty xterm-256color $'\n' install)"
assert_contains "$out" "¿En qué agentes instalar la extensión de Graphium?"
assert_not_contains "$out" "sin terminal no se pregunta"
assert_contains "$out" "Agentes elegidos:"
assert_file "$SCRATCH_HOME/$first_dir/extensions/graphium/index.ts"
assert_contains "$(cat "$SCRATCH_HOME/$first_dir/extensions/graphium/package.json")" '"version": "0.2.0"'

# 5) El mismo camino pipeado, con `agent` y cancelando: el checklist se ve y
# q sale sin instalar nada.
out="$(run_installer_piped_pty xterm-256color 'q' agent)"
assert_contains "$out" "¿En qué agentes instalar la extensión de Graphium?"
assert_contains "$out" "Selección cancelada: no se instaló nada."

# 6) Con una terminal tonta el checklist no va, pero el prompt numérico sí: se
# lee de /dev/tty y Enter omite.
out="$(run_installer_piped_pty dumb $'\n' agent)"
assert_contains "$out" "Agentes a instalar/actualizar"
assert_contains "$out" "No se instaló la extensión en ningún agente"
