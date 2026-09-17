#!/usr/bin/env bash
# G6.5 acceptance: scripts/review_launch.sh [model]  (default gemma4 via local Ollama)
set -euo pipefail
cd "$(dirname "$0")/.."
python3 scripts/review_launch.py "${1:-gemma4}"
