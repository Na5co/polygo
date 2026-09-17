#!/usr/bin/env bash
# G6.3 acceptance: a second model reads README.md and answers a fixed rubric.
# Every answer must contain the expected facts; exits 1 otherwise.
#   scripts/review_readme.sh [model]   (default: gemma4 via local Ollama)
set -euo pipefail
cd "$(dirname "$0")/.."
python3 scripts/review_readme.py "${1:-gemma4}"
