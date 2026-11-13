#!/usr/bin/env bash
# Builds the macOS release: the application bundle "MathCAS Notebook.app" and the disk image
# MathCAS-v2-<version>-<commit>-macos-<arch>.dmg that holds it.
#   MathCAS Notebook.app/Contents/MacOS/      mc-notebook, mc-docs, mc-repl, mc-kernel, mc-lsp, mc-docs-pages, mc-bench
#   MathCAS Notebook.app/Contents/Resources/  mathcas.icns
#   MathCAS Notebook.app/Contents/Info.plist  (the .mcn document type)
#   beside the bundle in the image: README.md, README-macos.txt, LICENSE, THIRD-PARTY-NOTICES.txt, CHANGELOG.md,
#   VERSION.txt, quickstart.pdf, examples/, editors/vscode/, jupyter/
# The programs look for one another next to themselves, so they all live in Contents/MacOS.
# Usage: packaging/macos/build-app.sh [OUTDIR]       (default OUTDIR: <target dir>/packages)
# Environment: CARGO_TARGET_DIR (default ./target), JOBS (default 3), MC_COMMIT (when the source is not a git
# checkout). The staged folder stays in <target dir>/package-stage (smoke.sh uses it). Prints the image's path last.
# The bundle is signed ad hoc (no developer identity): the first launch needs "Open" from the context menu.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
here="$root/packaging/macos"
target_dir="${CARGO_TARGET_DIR:-$root/target}"
jobs="${JOBS:-3}"
out="${1:-$target_dir/packages}"
cargo="${CARGO:-cargo}"

version="$(sed -n 's/^version = "\([^"]*\)".*/\1/p' "$root/Cargo.toml" | head -n 1)"
commit="${MC_COMMIT:-$(git -C "$root" rev-parse --short HEAD 2>/dev/null || echo local)}"
triple="$(rustc -vV | sed -n 's/^host: //p')"
case "$triple" in
  aarch64-apple-darwin) arch=arm64 ;;
  x86_64-apple-darwin) arch=x64 ;;
  *) echo "build-app.sh: unsupported host $triple" >&2; exit 2 ;;
esac
name="MathCAS-v2-$version-$commit-macos-$arch"
echo "[1/5 20%] building $name ($triple, $jobs jobs)"

bins=(mc-repl mc-kernel mc-lsp mc-notebook mc-docs mc-docs-pages mc-bench)
(cd "$root" && CARGO_TARGET_DIR="$target_dir" "$cargo" build --release --locked -j "$jobs" \
  -p mc-repl -p mc-engine -p mc-lsp -p mc-kernel -p mc-notebook -p mc-docs -p mc-bench)

echo "[2/5 40%] staging the bundle"
stage="$target_dir/package-stage"
top="$stage/$name"
app="$top/MathCAS Notebook.app"
rm -rf "$stage"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$out"
for b in "${bins[@]}"; do
  install -m 755 "$target_dir/release/$b" "$app/Contents/MacOS/$b"
  # keep the symbols (named frames in backtraces), drop the debug information
  strip -S "$app/Contents/MacOS/$b" 2>/dev/null || true
done
sed -e "s/@VERSION@/$version/g" "$here/Info.plist" > "$app/Contents/Info.plist"

# the icon: an .icns built from the 256-pixel picture
iconset="$stage/mathcas.iconset"
mkdir -p "$iconset"
for size in 16 32 128 256; do
  sips -z "$size" "$size" "$root/assets/app/icon_256.png" --out "$iconset/icon_${size}x${size}.png" >/dev/null
done
sips -z 32 32 "$root/assets/app/icon_256.png" --out "$iconset/icon_16x16@2x.png" >/dev/null
sips -z 64 64 "$root/assets/app/icon_256.png" --out "$iconset/icon_32x32@2x.png" >/dev/null
sips -z 256 256 "$root/assets/app/icon_256.png" --out "$iconset/icon_128x128@2x.png" >/dev/null
iconutil -c icns "$iconset" -o "$app/Contents/Resources/mathcas.icns"
rm -rf "$iconset"

echo "[3/5 60%] files beside the bundle"
install -m 644 "$here/README-macos.txt" "$top/README-macos.txt"
install -m 644 "$root/README.md" "$root/CHANGELOG.md" "$root/LICENSE" "$top/"
install -m 644 "$root/docs/guide/quickstart.pdf" "$top/quickstart.pdf"
cp -R "$root/examples" "$top/examples"
mkdir -p "$top/editors" "$top/jupyter"
cp -R "$root/editors/vscode" "$top/editors/vscode"
find "$root/editors/jupyter" -maxdepth 1 -type f -exec cp {} "$top/jupyter/" \;
echo "MathCAS v2 $version, commit $commit, built $(date -u +%Y-%m-%dT%H:%M:%SZ) for $triple" > "$top/VERSION.txt"
"$target_dir/release/mc-notices" --manifest-path "$root/Cargo.toml" --target "$triple" \
  --out "$top/THIRD-PARTY-NOTICES.txt"

echo "[4/5 80%] signing (ad hoc)"
codesign --force --deep --sign - "$app"
codesign --verify --deep --strict "$app"

echo "[5/5 100%] disk image"
dmg="$out/$name.dmg"
rm -f "$dmg"
hdiutil create -volname "MathCAS v2 $version" -srcfolder "$top" -ov -format UDZO "$dmg" >/dev/null
du -h "$dmg"
echo "$dmg"
