#!/usr/bin/env bash
set -euo pipefail

CODEX_HOME="${CODEX_HOME:-$HOME/.codex}"
mkdir -p "$CODEX_HOME"

TARGET="$CODEX_HOME/config.toml"
SOURCE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE_FILE="$SOURCE_DIR/config/fatherpaul/config.toml"

cp "$SOURCE_FILE" "$TARGET"
printf 'Wrote %s\n' "$TARGET"
printf 'Next steps:\n'
printf '  export FATHERPAUL_API_KEY=...\n'
printf '  printenv FATHERPAUL_API_KEY | fatherpaul-code login --with-api-key\n'
