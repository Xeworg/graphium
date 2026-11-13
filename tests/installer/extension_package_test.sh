#!/usr/bin/env bash
#
# package-release.sh: la extensión de Pi que viaja en el release dice la
# versión del release y lleva el contenido del árbol de trabajo.
#
# Sin el estampado, cada release publicaba el número que el checkout tuviera
# escrito (`0.1.0`) y el instalador mostraba esa cifra al lado de un binario
# nuevo, sin forma de distinguir una extensión vieja de una nueva.

set -euo pipefail
# shellcheck source=lib.sh
source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

new_scratch
trap cleanup_scratch EXIT

# Una versión que el checkout no declara, a propósito: si el paquete dijera la
# versión del árbol, el test pasaría sin estampar nada y no probaría lo que
# tiene que probar.
version="9.9.9"

# Un binario de mentira que reporta esa versión: el empaquetador verifica que
# el binario y el release digan lo mismo, y este test no necesita compilar
# nada para probarlo.
shim="$SCRATCH_HOME/graphium"
printf '#!/bin/sh\necho "graphium %s"\n' "$version" >"$shim"
chmod +x "$shim"

outdir="$SCRATCH_HOME/dist"
bash "$REPO_ROOT/scripts/package-release.sh" \
  --version "v$version" --arch x86_64 --outdir "$outdir" --bin "$shim" \
  >"$SCRATCH_HOME/package.log" 2>&1

tarball="$outdir/graphium-pi-extension.tar.gz"
assert_file "$tarball"

mkdir -p "$SCRATCH_HOME/extract"
tar -xzf "$tarball" -C "$SCRATCH_HOME/extract"

# El tarball trae el directorio `graphium/`, que es lo que el instalador
# extrae y copia (install_extension_from_release).
installed="$SCRATCH_HOME/extract/graphium"
assert_file "$installed/index.ts"
assert_file "$installed/package.json"

# La versión es la del release, no la del checkout.
assert_contains "$(cat "$installed/package.json")" "\"version\": \"$version\""

# Y el contenido es el del árbol: el index.ts de hoy registra el servidor con
# exposición directa, que es lo que hace que las tools lleguen al modelo.
assert_contains "$(cat "$installed/index.ts")" 'exposure: "direct"'

# El asset entra en el archivo de checksums que el instalador verifica.
assert_contains "$(cat "$outdir/SHA256SUMS")" "graphium-pi-extension.tar.gz"
