#!/usr/bin/env bash
#
# El registro de agentes es la única fuente de hosts: la ayuda publica nombre,
# variable de entorno y directorio por defecto, y --env, --all, la detección y
# la instalación usan exactamente esa lista. La elección se muestra antes de
# instalar.

set -euo pipefail
# shellcheck source=lib.sh
source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

new_scratch
trap cleanup_scratch EXIT

entries="$(registry_entries)"
if [[ -z "$entries" ]]; then
  printf 'FAIL: la ayuda no publica el registro de hosts\n'
  exit 1
fi

# La línea de --env lista exactamente los mismos nombres que el registro: ni
# uno de más ni uno de menos.
listed="$(usage_env_agents)"
if [[ -z "$listed" ]]; then
  printf 'FAIL: la ayuda no lista los agentes de --env\n'
  exit 1
fi
while IFS='|' read -r name _env_name _default_dir; do
  assert_contains "$listed" "$name"
done <<<"$entries"
for name in $listed; do
  found=0
  while IFS='|' read -r entry_name _env_name _default_dir; do
    [[ "$entry_name" == "$name" ]] && found=1
  done <<<"$entries"
  if ((found == 0)); then
    printf 'FAIL: %s aparece en --env pero no en el registro\n' "$name"
    exit 1
  fi
done

# Detección, elección e instalación, host por host y saliendo del registro.
while IFS='|' read -r name env_name default_dir; do
  base="$SCRATCH_HOME/$default_dir"

  # Host ausente: --env lo acepta, se omite y la elección se ve en pantalla
  # antes de instalar nada.
  out="$(run_installer agent --env "$name")"
  assert_contains "$out" "Agentes elegidos: $name"
  assert_contains "$out" "se omite"
  assert_no_file "$base/extensions/graphium"

  # Host presente (su directorio por defecto): la extensión aterriza ahí y el
  # resumen de la elección sale antes del renglón de instalación.
  mkdir -p "$base"
  out="$(run_installer agent --env "$name")"
  assert_contains "$out" "Extensión instalada"
  assert_file "$base/extensions/graphium/index.ts"
  summary_line="$(printf '%s\n' "$out" | grep -n 'Agentes elegidos:' | cut -d: -f1)"
  install_line="$(printf '%s\n' "$out" | grep -n 'Extensión instalada' | cut -d: -f1)"
  if ((${summary_line:-0} >= ${install_line:-0})); then
    printf 'FAIL: el resumen de la elección debe salir antes de instalar:\n%s\n' "$out"
    exit 1
  fi

  # La variable de entorno del registro manda sobre el directorio por defecto.
  override="$SCRATCH_HOME/override-$name"
  mkdir -p "$override"
  run_installer_with_env "$env_name=$override" agent --env "$name" >/dev/null
  assert_file "$override/extensions/graphium/index.ts"
done <<<"$entries"

# --all vuelve a pasar por todos los hosts del registro: sus directorios por
# defecto ya existen y ya tienen la extensión de la vuelta anterior, así que el
# mensaje es «actualizada», no «instalada» (T2 distingue las dos cosas).
out="$(run_installer agent --all)"
assert_contains "$out" "Extensión actualizada"
while IFS='|' read -r _name _env_name default_dir; do
  assert_file "$SCRATCH_HOME/$default_dir/extensions/graphium/index.ts"
done <<<"$entries"
