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
# Libraries the window toolkit loads by name when it starts (keyboard layouts under X11) and that a small system
# does not have: they travel in the image, with their copyright files, and AppRun puts them on the library path.
mkdir -p "$appdir/usr/lib" "$appdir/usr/share/doc/mathcas-v2/bundled-libraries"
for lib in libxkbcommon-x11.so.0 libxkbcommon.so.0 libxcb-xkb.so.1; do
  path="$(ldconfig -p | awk -v l="$lib" '$1 == l { print $NF; exit }')"
  if [ -z "$path" ]; then
    echo "build-appimage.sh: $lib is not installed here (packages libxkbcommon-x11-0, libxkbcommon0, libxcb-xkb1)" >&2
    exit 4
  fi
  cp -L "$path" "$appdir/usr/lib/$lib"
  ln -sf "$lib" "$appdir/usr/lib/${lib%.so.*}.so"
done
for pkg in libxkbcommon-x11-0 libxkbcommon0 libxcb-xkb1; do
  if [ -f "/usr/share/doc/$pkg/copyright" ]; then
    cp "/usr/share/doc/$pkg/copyright" "$appdir/usr/share/doc/mathcas-v2/bundled-libraries/$pkg.copyright"
  fi
done
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
# the libraries that travel in the image (build-appimage.sh), before the system's
export LD_LIBRARY_PATH="$here/usr/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
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
# Update information: where a tool such as AppImageUpdate finds the newest release and the file that lets it
# fetch only what changed (written beside the image as <image>.zsync when zsyncmake is installed).
update=()
if command -v zsyncmake >/dev/null 2>&1; then
  update=(-u "gh-releases-zsync|rajatmo|MathCASv2|latest|MathCAS-v2-*-linux-${name##*-linux-}.AppImage.zsync")
fi
(cd "$out" && ARCH="$arch" "$tool" --no-appstream ${update[@]+"${update[@]}"} "$appdir" "$image")
rm -rf "$appdir"
echo "$image"
