#!/usr/bin/env bash
# Builds the Linux release tarball MathCAS-v2-<version>-<commit>-linux-x64.tar.gz:
#   bin/          mc-notebook, mc-docs, mc-repl, mc-kernel, mc-lsp, mc-docs-pages, mc-bench (debug info stripped)
#   share/        .desktop files for the notebook and the documentation window, icons, the .mcn MIME type
#   README.md, README-linux.txt, LICENSE, THIRD-PARTY-NOTICES.txt (for this target), CHANGELOG.md, VERSION.txt,
#   quickstart.pdf, examples/, bench/cases/, editors/vscode/, jupyter/, install.sh (per user, no root needed)
# Usage: packaging/linux/build-tarball.sh [OUTDIR]       (default OUTDIR: <target dir>/packages)
# Environment: CARGO_TARGET_DIR (default ./target), JOBS (default 3), MC_COMMIT (when the source is not a git
# checkout), CARGO_OFFLINE=1 to build without network access. The staged folder stays in <target dir>/package-stage
# (packaging/linux/build-appimage.sh and smoke.sh use it). Prints the tarball's path last.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
here="$root/packaging/linux"
target_dir="${CARGO_TARGET_DIR:-$root/target}"
jobs="${JOBS:-3}"
out="${1:-$target_dir/packages}"
cargo="${CARGO:-cargo}"
offline=()
if [ "${CARGO_OFFLINE:-0}" = 1 ]; then offline=(--offline); fi

version="$(sed -n 's/^version = "\([^"]*\)".*/\1/p' "$root/Cargo.toml" | head -n 1)"
commit="${MC_COMMIT:-$(git -C "$root" rev-parse --short HEAD 2>/dev/null || echo local)}"
triple="$(rustc -vV | sed -n 's/^host: //p')"
case "$triple" in
  x86_64-*-linux-*) arch=x64 ;;
  aarch64-*-linux-*) arch=arm64 ;;
  *) echo "build-tarball.sh: unsupported host $triple" >&2; exit 2 ;;
esac
name="MathCAS-v2-$version-$commit-linux-$arch"
echo "[1/4 25%] building $name ($triple, $jobs jobs)"

bins=(mc-repl mc-kernel mc-lsp mc-notebook mc-docs mc-docs-pages mc-bench)
(cd "$root" && CARGO_TARGET_DIR="$target_dir" "$cargo" build --release --locked "${offline[@]}" -j "$jobs" \
  -p mc-repl -p mc-engine -p mc-lsp -p mc-kernel -p mc-notebook -p mc-docs -p mc-bench)

echo "[2/4 50%] staging"
stage="$target_dir/package-stage"
top="$stage/$name"
rm -rf "$stage"
mkdir -p "$top/bin" "$top/share/applications" "$top/share/mime/packages" "$out"
for b in "${bins[@]}"; do
  install -m 755 "$target_dir/release/$b" "$top/bin/$b"
  # keep the symbols (named frames in backtraces), drop the line tables
  if command -v strip >/dev/null; then strip --strip-debug "$top/bin/$b"; fi
done
install -m 644 "$here/mathcas-notebook.desktop" "$here/mathcas-docs.desktop" "$top/share/applications/"
install -m 644 "$here/mathcas-mime.xml" "$top/share/mime/packages/mathcas.xml"
for size in 128 256; do
  mkdir -p "$top/share/icons/hicolor/${size}x$size/apps"
  install -m 644 "$root/assets/app/icon_$size.png" "$top/share/icons/hicolor/${size}x$size/apps/mathcas.png"
done
install -m 755 "$here/install.sh" "$top/install.sh"
install -m 644 "$here/README-linux.txt" "$top/README-linux.txt"
install -m 644 "$root/README.md" "$root/CHANGELOG.md" "$root/LICENSE" "$top/"
install -m 644 "$root/docs/guide/quickstart.pdf" "$top/quickstart.pdf"
cp -r "$root/examples" "$top/examples"
mkdir -p "$top/bench" "$top/editors" "$top/jupyter"
cp -r "$root/bench/cases" "$top/bench/cases"
cp -r "$root/editors/vscode" "$top/editors/vscode"
find "$root/editors/jupyter" -maxdepth 1 -type f -exec cp {} "$top/jupyter/" \;
echo "MathCAS v2 $version, commit $commit, built $(date -u +%Y-%m-%dT%H:%M:%SZ) for $triple" > "$top/VERSION.txt"

echo "[3/4 75%] third-party notices"
"$target_dir/release/mc-notices" --manifest-path "$root/Cargo.toml" --target "$triple" "${offline[@]}" \
  --out "$top/THIRD-PARTY-NOTICES.txt"

echo "[4/4 100%] archiving"
tarball="$out/$name.tar.gz"
tar -C "$stage" --sort=name --owner=0 --group=0 --numeric-owner -czf "$tarball" "$name"
du -h "$tarball"
echo "$tarball"
