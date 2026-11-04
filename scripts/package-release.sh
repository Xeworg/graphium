#!/usr/bin/env bash
# El empaquetado del release: un artefacto por plataforma y sus checksums.
#
# El workflow de GitHub Actions llama este script una vez por plataforma y
# después une los SHA256SUMS en el único que publica el release. Los nombres
# de los assets son estables entre versiones (el tag ya les pone namespace):
#
#   https://github.com/Xeworg/graphium/releases/latest/download/graphium-linux-x86_64.tar.gz
#   https://github.com/Xeworg/graphium/releases/latest/download/graphium-pi-extension.tar.gz
#
# El archivo del binario lleva el binario, la licencia, el readme y un
# VERSION: el instalador compara VERSION con la versión pedida para negarse
# a instalar un artefacto que no corresponde. La extensión de Pi viaja
# aparte (graphium-pi-extension.tar.gz, con el directorio graphium/ adentro)
# porque es la misma para todas las plataformas.
set -uo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"

VERSION=""
ARCH=""
OUTDIR="$REPO_ROOT/dist"
BIN=""

usage() {
  printf '%s\n' \
    'Usage: scripts/package-release.sh --version <semver> [options]' \
    '' \
    'Options:' \
    '  --version <semver>   version to package (e.g. 0.1.0 or v0.1.0) [required]' \
    '  --arch <arch>        x86_64 or aarch64 [default: the host architecture]' \
    '  --outdir <dir>       where the artifact is written [default: ./dist]' \
    '  --bin <path>         the binary to package [default: target/release/graphium]' \
    '  --no-extension       skip the graphium-pi-extension.tar.gz asset' \
    '  -h, --help           show this help'
}

die() {
  printf 'package-release.sh: %s\n' "$*" >&2
  exit 2
}

detect_arch() {
  case "$(uname -m)" in
    x86_64 | amd64) printf 'x86_64' ;;
    aarch64 | arm64) printf 'aarch64' ;;
    *) printf 'unknown' ;;
  esac
}

while (($# > 0)); do
  case "$1" in
    --version)
      (($# >= 2)) || die "--version needs a value"
      VERSION="$2"
      shift 2
      ;;
    --arch)
      (($# >= 2)) || die "--arch needs a value"
      ARCH="$2"
      shift 2
      ;;
    --outdir)
      (($# >= 2)) || die "--outdir needs a value"
      OUTDIR="$2"
      shift 2
      ;;
    --bin)
      (($# >= 2)) || die "--bin needs a value"
      BIN="$2"
      shift 2
      ;;
    --no-extension)
      SKIP_EXTENSION=1
      shift
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      printf 'package-release.sh: unknown argument: %s\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

[[ -n "$VERSION" ]] || {
  usage >&2
  die "--version is required"
}
# The tag may carry a leading "v"; the version itself never does.
VERSION="${VERSION#v}"
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$ ]] ||
  die "'$VERSION' is not a semantic version"

[[ -n "$ARCH" ]] || ARCH="$(detect_arch)"
case "$ARCH" in
  x86_64 | aarch64) ;;
  *) die "unsupported architecture '$ARCH' (expected x86_64 or aarch64)" ;;
esac

[[ -n "$BIN" ]] || BIN="$REPO_ROOT/target/release/graphium"
[[ -f "$BIN" ]] || die "the binary was not found at $BIN (build the release profile first)"
[[ -x "$BIN" ]] || die "$BIN is not executable"

# The packaged binary must be the version being released: packaging 0.2.0
# with a 0.1.0 binary is the kind of mistake nobody notices until a user
# reports the wrong version.
binary_version="$("$BIN" --version 2>/dev/null | awk '{print $NF}' || true)"
if [[ -n "$binary_version" && "$binary_version" != "$VERSION" ]]; then
  die "the binary reports version '$binary_version' but '$VERSION' was requested"
fi

[[ -f "$REPO_ROOT/extensions/pi/graphium/index.ts" ]] ||
  die "the pi extension was not found at extensions/pi/graphium"

mkdir -p -- "$OUTDIR"
OUTDIR="$(cd -- "$OUTDIR" && pwd)"

add_checksum() {
  local file="$1" name
  name="$(basename -- "$file")"
  local checksum
  checksum="$(sha256sum -- "$file" | awk '{print $1}')"
  local checksums="$OUTDIR/SHA256SUMS"
  if [[ -f "$checksums" ]]; then
    grep -v -- " $name\$" "$checksums" >"$checksums.tmp" || true
    mv -f -- "$checksums.tmp" "$checksums"
  fi
  printf '%s  %s\n' "$checksum" "$name" >>"$checksums"
  printf '  %s  %s\n' "$checksum" "$name"
}

# --- the binary artifact -----------------------------------------------------
stage="$(mktemp -d)"
trap 'rm -rf -- "$stage"' EXIT

install -m 0755 -- "$BIN" "$stage/graphium"
install -m 0644 -- "$REPO_ROOT/LICENSE" "$stage/LICENSE"
install -m 0644 -- "$REPO_ROOT/README.md" "$stage/README.md"
printf '%s\n' "$VERSION" >"$stage/VERSION"

asset="graphium-linux-$ARCH.tar.gz"
tar -czf "$OUTDIR/$asset" -C "$stage" graphium LICENSE README.md VERSION
printf 'Packaged %s (%s)\n' "$asset" "$VERSION"
add_checksum "$OUTDIR/$asset"

# --- the pi extension artifact ----------------------------------------------
# Platform-independent, so the workflow packages it once (the x86_64 job);
# --no-extension skips it for the other platforms.
if [[ "${SKIP_EXTENSION:-0}" != "1" ]]; then
  rm -rf -- "$stage/graphium"
  cp -R -- "$REPO_ROOT/extensions/pi/graphium" "$stage/graphium"
  ext_asset="graphium-pi-extension.tar.gz"
  tar -czf "$OUTDIR/$ext_asset" -C "$stage" graphium
  printf 'Packaged %s\n' "$ext_asset"
  add_checksum "$OUTDIR/$ext_asset"
fi
