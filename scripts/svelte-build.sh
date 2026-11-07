#!/usr/bin/env bash
# Builds the Svelte frontend of Graphium (`crates/gx-app/frontend-svelte`).
#
# This only produces `crates/gx-app/frontend-svelte/dist/`: it never touches
# the vanilla frontend (`crates/gx-app/frontend/`), which stays the default
# `frontendDist` of `crates/gx-app/tauri.conf.json` and the one the Rust tests
# pin with `include_str!`.
#
# Usage:
#   scripts/svelte-build.sh            # install (if needed) + build
#   scripts/svelte-build.sh --clean    # reinstall from the lockfile first
#
# Output: `crates/gx-app/frontend-svelte/dist/` plus the size of the bundle,
# which is one of the numbers the A/B measurement (T5 of `odd/tasks/svelte-ui.md`)
# reports.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
app="$root/crates/gx-app/frontend-svelte"

clean=0
for argument in "$@"; do
  case "$argument" in
    --clean) clean=1 ;;
    *)
      echo "svelte-build: unknown argument: $argument" >&2
      exit 2
      ;;
  esac
done

if ! command -v npm >/dev/null 2>&1; then
  echo "svelte-build: npm is not on PATH" >&2
  exit 1
fi

cd "$app"

# `npm ci` is the reproducible door: it needs the lockfile and wipes
# `node_modules`. Without a lockfile (or with `--clean` unset and the modules
# already there) the plain install is enough.
if [ "$clean" = 1 ] || [ ! -d node_modules ]; then
  if [ -f package-lock.json ]; then
    npm ci --no-fund --no-audit
  else
    npm install --no-fund --no-audit
  fi
fi

npm run build

echo
echo "svelte-build: dist ready at crates/gx-app/frontend-svelte/dist"
du -sh dist 2>/dev/null || true
find dist -type f | wc -l | xargs printf 'svelte-build: %s files\n'
