#!/bin/sh
# Installs the latest kraken release: curl -fsSL https://raw.githubusercontent.com/EliTheSurfer/kraken/main/install.sh | sh
set -eu

repo="EliTheSurfer/kraken"
install_dir="${KRAKEN_INSTALL_DIR:-$HOME/.local/bin}"

case "$(uname -s)" in
  Darwin) os="apple-darwin" ;;
  Linux) os="unknown-linux-gnu" ;;
  *) echo "kraken: unsupported system $(uname -s), build it with: cargo install --git https://github.com/$repo" >&2; exit 1 ;;
esac

case "$(uname -m)" in
  arm64|aarch64) arch="aarch64" ;;
  x86_64|amd64) arch="x86_64" ;;
  *) echo "kraken: unsupported architecture $(uname -m)" >&2; exit 1 ;;
esac

target="$arch-$os"
url="https://github.com/$repo/releases/latest/download/kraken-$target.tar.gz"
workdir="$(mktemp -d)"
trap 'rm -rf "$workdir"' EXIT

echo "Downloading kraken for $target..."
curl -fsSL "$url" -o "$workdir/kraken.tar.gz"
tar -xzf "$workdir/kraken.tar.gz" -C "$workdir"
mkdir -p "$install_dir"
install -m 0755 "$workdir/kraken" "$install_dir/kraken"
echo "Installed kraken to $install_dir/kraken"

case ":$PATH:" in
  *":$install_dir:"*) ;;
  *) echo "Add $install_dir to your PATH to run kraken from anywhere." ;;
esac
