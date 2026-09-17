#!/usr/bin/env bash
# Re-record docs/demo.gif. Needs: vhs, ffmpeg, ollama with qwen3:8b pulled.
#   scripts/demo.sh
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release -q
stage=$(mktemp -d)
cp -R docs/demo/App "$stage/App"
export POLYGO_DEMO_DIR="$stage"
export PATH="$PWD/target/release:$PATH"
rm -rf docs/demo-frames
vhs docs/demo.tape
ffmpeg -y -loglevel error -framerate 12 -i docs/demo-frames/frame-text-%05d.png \
  -vf "scale=900:-1:flags=lanczos,palettegen=max_colors=128" docs/demo-palette.png
ffmpeg -y -loglevel error -framerate 12 -i docs/demo-frames/frame-text-%05d.png \
  -i docs/demo-palette.png \
  -lavfi "scale=900:-1:flags=lanczos[x];[x][1:v]paletteuse=dither=bayer:bayer_scale=5" \
  docs/demo.gif
rm -rf docs/demo-frames docs/demo-palette.png "$stage"
ls -la docs/demo.gif
