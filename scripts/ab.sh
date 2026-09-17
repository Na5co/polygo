#!/usr/bin/env bash
# G4.4 wrapper: scripts/ab.sh <repo> <catalog.xcstrings> <locale> [--n N] [--model M] [--judge J]
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build -q --release
python3 scripts/ab.py "$@"
