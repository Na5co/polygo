#!/usr/bin/env bash
# G0.1 wrapper: scripts/killtest.sh <file.xcstrings> <locale> [extra args for killtest.py]
set -euo pipefail
cd "$(dirname "$0")/.."
python3 scripts/killtest.py "$@"
