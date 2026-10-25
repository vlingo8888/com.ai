#!/bin/sh
# Installs the com CLI on Linux (macOS: the .dmg / .pkg) without Node, npm or Bun:
#   curl -fsSL https://hcm04.vstorage.vngcloud.vn/files/com-cli/install.sh | sh
# COM_VERSION=1.0.2 for a given release, COM_INSTALL_DIR to install elsewhere than ~/.com/cli.
set -eu

base="${COM_DOWNLOAD_URL:-https://hcm04.vstorage.vngcloud.vn/files/com-cli}"
release="latest"
[ -n "${COM_VERSION:-}" ] && release="v${COM_VERSION#v}"
dest="${COM_INSTALL_DIR:-$HOME/.com/cli}"

case "$(uname -s)" in
  Linux) ;;
  Darwin) echo "On macOS, install com with the .dmg: $base/$release/com-macos.dmg" >&2; exit 1 ;;
  *) echo "Unsupported system: $(uname -s). On Windows, use $base/$release/com-windows-setup.exe" >&2; exit 1 ;;
esac
case "$(uname -m)" in
  x86_64 | amd64) arch=x64 ;;
  aarch64 | arm64) arch=arm64 ;;
  *) echo "Unsupported CPU: $(uname -m)" >&2; exit 1 ;;
esac
file="com-linux-$arch.tar.gz"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
echo "Downloading $base/$release/$file"
curl -fL --progress-bar "$base/$release/$file" -o "$tmp/$file"
curl -fsSL "$base/$release/SHA256SUMS" -o "$tmp/SHA256SUMS"
expected=$(grep " $file\$" "$tmp/SHA256SUMS" | cut -d' ' -f1)
if command -v sha256sum >/dev/null 2>&1; then
  actual=$(sha256sum "$tmp/$file" | cut -d' ' -f1)
else
  actual=$(shasum -a 256 "$tmp/$file" | cut -d' ' -f1)
fi
if [ -z "$expected" ] || [ "$expected" != "$actual" ]; then
  echo "Checksum mismatch for $file" >&2
  exit 1
fi

tar -xzf "$tmp/$file" -C "$tmp"
rm -rf "$dest"
mkdir -p "$(dirname "$dest")"
mv "$tmp/com" "$dest"

line="export PATH=\"$dest:\$PATH\""
for rc in "$HOME/.bashrc" "$HOME/.zshrc" "$HOME/.profile"; do
  [ -f "$rc" ] || continue
  grep -qF "$dest" "$rc" || printf '\n# com CLI\n%s\n' "$line" >>"$rc"
done

"$dest/com" --version >/dev/null 2>&1 || echo "Warning: $dest/com does not start on this machine" >&2
echo "Installed com into $dest"
case ":$PATH:" in
  *":$dest:"*) ;;
  *) echo "Open a new terminal, or run: $line" ;;
esac
