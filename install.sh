#!/usr/bin/env sh
# Install the latest polygo release into ~/.local/bin (or $POLYGO_INSTALL_DIR).
#   curl -fsSL https://raw.githubusercontent.com/atanasa/polygo/main/install.sh | sh
set -eu
repo="atanasa/polygo"
dir="${POLYGO_INSTALL_DIR:-$HOME/.local/bin}"
os=$(uname -s | tr '[:upper:]' '[:lower:]')
arch=$(uname -m)
case "$arch" in
  x86_64|amd64) arch=x86_64 ;;
  aarch64|arm64) arch=aarch64 ;;
  *) echo "unsupported architecture: $arch" >&2; exit 1 ;;
esac
case "$os" in darwin|linux) ;; *) echo "unsupported OS: $os (use the Windows zip from the releases page)" >&2; exit 1 ;; esac
tag="${POLYGO_VERSION:-$(curl -fsSL "https://api.github.com/repos/$repo/releases/latest" | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p')}"
[ -n "$tag" ] || { echo "could not determine the latest release" >&2; exit 1; }
url="https://github.com/$repo/releases/download/$tag/polygo-$tag-$arch-$os.tar.gz"
tmp=$(mktemp -d)
echo "downloading $url"
curl -fsSL "$url" | tar -xz -C "$tmp"
mkdir -p "$dir"
mv "$tmp/polygo" "$dir/polygo"
chmod +x "$dir/polygo"
rm -rf "$tmp"
echo "installed $dir/polygo ($tag)"
case ":$PATH:" in *":$dir:"*) ;; *) echo "add $dir to your PATH" ;; esac
