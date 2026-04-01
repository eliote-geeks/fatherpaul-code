#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CLI_DIR="$ROOT_DIR/codex-cli"
VERSION="${1:?usage: package-fatherpaul-code-artifacts.sh <version> [vendor-src-dir] [output-dir]}"
VENDOR_SRC="${2:-$ROOT_DIR/dist/fatherpaul-code/vendor-src}"
OUTPUT_DIR="${3:-$ROOT_DIR/dist/fatherpaul-code/packages}"

mkdir -p "$OUTPUT_DIR"

python3 "$CLI_DIR/scripts/build_npm_package.py" \
  --package codex \
  --version "$VERSION" \
  --vendor-src "$VENDOR_SRC" \
  --pack-output "$OUTPUT_DIR/fatherpaul-code-$VERSION.tgz"

for platform_package in codex-linux-x64 codex-win32-x64; do
  artifact_name="${platform_package/codex/fatherpaul-code}-$VERSION.tgz"
  python3 "$CLI_DIR/scripts/build_npm_package.py" \
    --package "$platform_package" \
    --version "$VERSION" \
    --vendor-src "$VENDOR_SRC" \
    --pack-output "$OUTPUT_DIR/$artifact_name"
done

echo "Packaged artifacts in $OUTPUT_DIR"
