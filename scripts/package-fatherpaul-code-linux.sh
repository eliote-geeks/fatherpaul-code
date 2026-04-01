#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUST_DIR="$ROOT_DIR/codex-rs"
CLI_DIR="$ROOT_DIR/codex-cli"
DIST_DIR="$ROOT_DIR/dist/fatherpaul-code"
VERSION="${1:-0.0.0-dev}"
TARGET_TRIPLE="${TARGET_TRIPLE:-x86_64-unknown-linux-musl}"
VENDOR_SRC="$DIST_DIR/vendor-src"

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "This packaging helper currently supports Linux only." >&2
  exit 1
fi

if ! command -v rg >/dev/null 2>&1; then
  echo "rg is required on PATH to bundle FatherPaul Code." >&2
  exit 1
fi

source /root/.cargo/env

mkdir -p "$DIST_DIR"
rm -rf "$VENDOR_SRC"
mkdir -p "$VENDOR_SRC/$TARGET_TRIPLE/codex" "$VENDOR_SRC/$TARGET_TRIPLE/path"

echo "==> Building FatherPaul Code release binary"
cargo build -p codex-cli --release --target "$TARGET_TRIPLE"

echo "==> Hydrating vendor payload"
cp "$RUST_DIR/target/$TARGET_TRIPLE/release/codex" "$VENDOR_SRC/$TARGET_TRIPLE/codex/codex"
cp "$(command -v rg)" "$VENDOR_SRC/$TARGET_TRIPLE/path/rg"
chmod +x "$VENDOR_SRC/$TARGET_TRIPLE/codex/codex" "$VENDOR_SRC/$TARGET_TRIPLE/path/rg"

echo "==> Packing npm tarball"
python3 "$CLI_DIR/scripts/build_npm_package.py" \
  --package codex \
  --version "$VERSION" \
  --vendor-src "$VENDOR_SRC" \
  --pack-output "$DIST_DIR/fatherpaul-code-$VERSION.tgz"

echo "==> Done"
echo "$DIST_DIR/fatherpaul-code-$VERSION.tgz"
