#!/usr/bin/env bash
#
# El camino publicado, ejercitado localmente: una versión fijada
# (GRAPHIUM_VERSION o --version) baja ese release exacto del binario **y** de la
# extensión, y no `/latest`.
#
# El «servidor» es un directorio con los mismos nombres de asset servido por
# `file://`: el instalador no sabe la diferencia, así que este test prueba el
# código real del modo pipeado sin red y sin publicar nada.
#
# Antes de este test, `install` y `agent` parseaban la versión y después la
# descartaban: las dos descargas iban a `/latest/download`, que en un release
# fijado es una versión distinta —o ninguna.

set -euo pipefail
# shellcheck source=lib.sh
source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

new_scratch
trap cleanup_scratch EXIT

version="0.2.0"
release="$SCRATCH_HOME/release"
stage="$SCRATCH_HOME/stage"
mkdir -p "$release/download/v$version" "$stage/bin" "$stage/ext"

# --- el binario del release --------------------------------------------------
printf '#!/bin/sh\necho "graphium %s"\n' "$version" >"$stage/bin/graphium"
chmod +x "$stage/bin/graphium"
printf 'MIT\n' >"$stage/bin/LICENSE"
printf '# Graphium\n' >"$stage/bin/README.md"
printf '%s\n' "$version" >"$stage/bin/VERSION"
tar -czf "$release/download/v$version/graphium-linux-x86_64.tar.gz" -C "$stage/bin" \
  graphium LICENSE README.md VERSION

# --- la extensión del release ------------------------------------------------
mkdir -p "$stage/ext/graphium"
printf 'export default function graphium() { /* %s */ }\n' "$version" \
  >"$stage/ext/graphium/index.ts"
printf '{\n  "name": "@xeworg/graphium-pi",\n  "version": "%s"\n}\n' "$version" \
  >"$stage/ext/graphium/package.json"
tar -czf "$release/download/v$version/graphium-pi-extension.tar.gz" -C "$stage/ext" graphium

# --- los checksums, como los publica el release ------------------------------
(
  cd "$release/download/v$version"
  sha256sum -- graphium-linux-x86_64.tar.gz graphium-pi-extension.tar.gz >SHA256SUMS
)

# --- instalación pipeada, con la versión fijada ------------------------------
piped() {
  env -u PI_CODING_AGENT_DIR -u GENTLE_SHELL_AGENT_DIR \
    HOME="$SCRATCH_HOME" CARGO_HOME="$REAL_CARGO_HOME" \
    GRAPHIUM_RELEASES_URL="file://$release" GRAPHIUM_VERSION="$version" \
    bash -s -- "$@" <"$INSTALLER"
}

# Un host de Pi presente en el HOME de scratch, para el camino de la extensión.
mkdir -p "$SCRATCH_HOME/.pi/agent"

out="$(piped install 2>&1)"
assert_contains "$out" "download/v$version/graphium-linux-x86_64.tar.gz"
assert_not_contains "$out" "latest/download"
assert_file "$SCRATCH_HOME/.local/bin/graphium"
assert_contains "$("$SCRATCH_HOME/.local/bin/graphium" --version)" "$version"

out="$(piped agent --env pi 2>&1)"
assert_contains "$out" "download/v$version/graphium-pi-extension.tar.gz"
assert_not_contains "$out" "latest/download"
assert_file "$SCRATCH_HOME/.pi/agent/extensions/graphium/index.ts"
assert_contains "$(cat "$SCRATCH_HOME/.pi/agent/extensions/graphium/package.json")" "\"version\": \"$version\""

# Y `agent --version` fija la misma versión sin depender del entorno.
rm -rf "$SCRATCH_HOME/.pi/agent/extensions/graphium"
out="$(env -u PI_CODING_AGENT_DIR -u GENTLE_SHELL_AGENT_DIR \
  HOME="$SCRATCH_HOME" CARGO_HOME="$REAL_CARGO_HOME" \
  GRAPHIUM_RELEASES_URL="file://$release" \
  bash -s -- agent --env pi --version "$version" <"$INSTALLER" 2>&1)"
assert_contains "$out" "download/v$version/graphium-pi-extension.tar.gz"
assert_file "$SCRATCH_HOME/.pi/agent/extensions/graphium/index.ts"
