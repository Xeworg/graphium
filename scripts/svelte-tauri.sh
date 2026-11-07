#!/usr/bin/env bash
# Builds (and optionally runs) Graphium with the Svelte frontend, without
# touching the default vanilla path.
#
# How the switch works: `tauri.conf.json` stays as it is (`frontendDist:
# "frontend"`, the vanilla page the Rust tests pin with `include_str!`). The
# build script of `gx-app` (`tauri-build`) and the codegen of
# `tauri::generate_context!()` both merge the `TAURI_CONFIG` environment
# variable into the parsed config with a JSON merge patch, and both emit
# `cargo:rerun-if-env-changed=TAURI_CONFIG`, so switching the variable forces a
# rebuild of the crate and nothing else.
#
# The path of `frontendDist` is resolved against the directory of the config
# (`crates/gx-app`), so `frontend-svelte/dist` is the Svelte bundle.
#
# Usage:
#   scripts/svelte-tauri.sh              # build target/debug/graphium (Svelte)
#   scripts/svelte-tauri.sh --run        # build and run it (solo-ui mode)
#   scripts/svelte-tauri.sh --vanilla    # build the default (vanilla) binary
#
# Evidence that the switch is active: the embedded bundle carries the Svelte
# chunk name, e.g.
#   strings -a target/debug/graphium | grep -o 'assets/index-[A-Za-z0-9_-]*\.js'
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"

run=0
config='{"build":{"frontendDist":"frontend-svelte/dist"}}'
for argument in "$@"; do
  case "$argument" in
    --run) run=1 ;;
    --vanilla) config="" ;;
    *)
      echo "svelte-tauri: unknown argument: $argument" >&2
      exit 2
      ;;
  esac
done

if [ -n "$config" ]; then
  if [ ! -f crates/gx-app/frontend-svelte/dist/index.html ]; then
    echo "svelte-tauri: no svelte dist yet, building it first" >&2
    "$root/scripts/svelte-build.sh"
  fi
  echo "svelte-tauri: TAURI_CONFIG=$config"
  TAURI_CONFIG="$config" cargo build -p gx-app
else
  echo "svelte-tauri: default config (vanilla frontend)"
  env -u TAURI_CONFIG cargo build -p gx-app
fi

if [ "$run" = 1 ]; then
  exec "$root/target/debug/graphium"
fi
