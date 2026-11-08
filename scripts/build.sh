#!/usr/bin/env bash
# Builds Graphium: the frontend bundle first, then the binary that embeds it.
#
# The plan is a Svelte app in `crates/gx-app/frontend`. `tauri::generate_context!()`
# embeds `frontendDist` (`frontend/dist`) at compile time, so the Vite build has
# to run BEFORE the cargo build: a plain `cargo build` on a clean checkout stops
# with "the `frontendDist` configuration is set to `frontend/dist` but this path
# doesn't exist". This script is that order in one place — the same one CI runs.
#
# Usage:
#   scripts/build.sh              # npm ci (when needed) + npm run build + cargo build -p gx-app
#   scripts/build.sh --clean      # reinstall from the lockfile first (`npm ci`)
#   scripts/build.sh --run        # build and run target/debug/graphium
#   scripts/build.sh --release    # build the release binary instead
#
# Output: `crates/gx-app/frontend/dist/` and `target/{debug,release}/graphium`.
# `dist/` and `node_modules/` stay out of git (see the frontend .gitignore).
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
app="$root/crates/gx-app/frontend"

profile=debug
cargo_args=(-p gx-app)
run=0
clean=0
for argument in "$@"; do
  case "$argument" in
    --clean) clean=1 ;;
    --run) run=1 ;;
    --release) profile=release; cargo_args+=(--release) ;;
    *)
      echo "build: unknown argument: $argument" >&2
      exit 2
      ;;
  esac
done

for tool in npm cargo; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "build: $tool is not on PATH" >&2
    exit 1
  fi
done

cd "$app"

# `npm ci` is the reproducible door: it needs the lockfile and wipes
# `node_modules`. Without `--clean` and with the modules already there, the
# install is skipped.
if [ "$clean" = 1 ] || [ ! -d node_modules ]; then
  if [ -f package-lock.json ]; then
    npm ci --no-fund --no-audit
  else
    npm install --no-fund --no-audit
  fi
fi

npm run build

cd "$root"
cargo build "${cargo_args[@]}"

if [ "$run" = 1 ]; then
  exec "$root/target/$profile/graphium"
fi
