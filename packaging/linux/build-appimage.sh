#!/usr/bin/env bash
# Builds an AppImage from the staged package of build-tarball.sh (no root needed):
#   MathCAS-v2-<version>-<commit>-linux-x64.AppImage    runs the notebook; "<AppImage> repl|docs|kernel|lsp|docs-pages|bench
#                                                       [args]" runs that program instead
# Usage: packaging/linux/build-appimage.sh STAGED_DIR [OUTDIR]
#   STAGED_DIR: <target dir>/package-stage/MathCAS-v2-... (or an unpacked tarball); OUTDIR defaults to its parent.
# Needs appimagetool (APPIMAGETOOL, or on the PATH); without it the AppDir is left next to the output and the
# script says so (exit 3). The programs load the system's graphics libraries (Wayland or X11, EGL/OpenGL or Vulkan)
# at run time, as from the tarball; nothing else is bundled.
set -euo pipefail

staged="$(cd "${1:?usage: build-appimage.sh STAGED_DIR [OUTDIR]}" && pwd)"
out="$(mkdir -p "${2:-$(dirname "$staged")}" && cd "${2:-$(dirname "$staged")}" && pwd)"
name="$(basename "$staged")"
appdir="$out/$name.AppDir"

rm -rf "$appdir"
mkdir -p "$appdir/usr"
cp -r "$staged/bin" "$staged/share" "$appdir/usr/"
mkdir -p "$appdir/usr/share/doc/mathcas-v2"
for f in README.md README-linux.txt LICENSE THIRD-PARTY-NOTICES.txt CHANGELOG.md VERSION.txt quickstart.pdf; do
  cp "$staged/$f" "$appdir/usr/share/doc/mathcas-v2/"
done
cp -r "$staged/examples" "$appdir/usr/share/doc/mathcas-v2/examples"
# the AppImage's own entry: the notebook, with the program name AppRun dispatches on
sed -e 's|^Exec=mc-notebook|Exec=mathcas|' -e '/^TryExec=/d' "$staged/share/applications/mathcas-notebook.desktop" \
  > "$appdir/mathcas.desktop"
cp "$staged/share/icons/hicolor/256x256/apps/mathcas.png" "$appdir/mathcas.png"
ln -s mathcas.png "$appdir/.DirIcon"
cat > "$appdir/AppRun" <<'APPRUN'
#!/bin/sh
# Runs the notebook, or the program named by the first argument (repl, docs, kernel, lsp, docs-pages, bench).
here="$(dirname "$(readlink -f "$0")")"
program=mc-notebook
case "${1:-}" in
  repl|docs|kernel|lsp|docs-pages|bench) program="mc-$1"; shift ;;
esac
exec "$here/usr/bin/$program" "$@"
APPRUN
chmod 755 "$appdir/AppRun"

tool="${APPIMAGETOOL:-$(command -v appimagetool || true)}"
if [ -z "$tool" ]; then
  echo "appimagetool not found (set APPIMAGETOOL or put it on the PATH); the AppDir is at $appdir" >&2
  exit 3
fi
image="$out/$name.AppImage"
case "$name" in *-linux-x64) arch=x86_64 ;; *-linux-arm64) arch=aarch64 ;; *) arch="$(uname -m)" ;; esac
ARCH="$arch" "$tool" --no-appstream "$appdir" "$image"
rm -rf "$appdir"
echo "$image"
