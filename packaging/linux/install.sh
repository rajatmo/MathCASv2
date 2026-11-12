#!/usr/bin/env bash
# Installs this MathCAS v2 folder for the current user (no root): the programs into PREFIX/lib/mathcas-v2 with
# links in PREFIX/bin, the menu entries, icons and the .mcn file type. PREFIX defaults to ~/.local.
#   ./install.sh               install (again: replaces an earlier install)
#   ./install.sh --uninstall   remove everything this script installed
# PREFIX=/usr/local sudo ./install.sh installs for all users.
set -euo pipefail

src="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
prefix="${PREFIX:-$HOME/.local}"
app="$prefix/lib/mathcas-v2"
bins=(mc-repl mc-kernel mc-lsp mc-notebook mc-docs mc-docs-pages mc-bench)
sizes=(128 256)

refresh() {
  if command -v update-desktop-database >/dev/null; then
    update-desktop-database -q "$prefix/share/applications" || true
  fi
  if command -v update-mime-database >/dev/null; then
    update-mime-database "$prefix/share/mime" >/dev/null || true
  fi
  if command -v gtk-update-icon-cache >/dev/null; then
    gtk-update-icon-cache -q -t "$prefix/share/icons/hicolor" || true
  fi
}

if [ "${1:-}" = "--uninstall" ]; then
  for b in "${bins[@]}"; do
    if [ -L "$prefix/bin/$b" ]; then rm -f "$prefix/bin/$b"; fi
  done
  rm -f "$prefix/share/applications/mathcas-notebook.desktop" "$prefix/share/applications/mathcas-docs.desktop"
  rm -f "$prefix/share/mime/packages/mathcas.xml"
  for size in "${sizes[@]}"; do rm -f "$prefix/share/icons/hicolor/${size}x$size/apps/mathcas.png"; done
  rm -rf "$app"
  refresh
  echo "MathCAS v2 removed from $prefix"
  exit 0
fi

if [ "$src" != "$app" ]; then
  rm -rf "$app"
  mkdir -p "$app"
  cp -r "$src/." "$app/"
fi
mkdir -p "$prefix/bin" "$prefix/share/applications" "$prefix/share/mime/packages"
for b in "${bins[@]}"; do ln -sf "$app/bin/$b" "$prefix/bin/$b"; done
for f in mathcas-notebook.desktop mathcas-docs.desktop; do
  # the menu entries name the installed programs by full path: they work without PREFIX/bin on the PATH
  sed -e "s|^Exec=mc-|Exec=$app/bin/mc-|" -e "s|^TryExec=mc-|TryExec=$app/bin/mc-|" \
    "$app/share/applications/$f" > "$prefix/share/applications/$f"
done
cp "$app/share/mime/packages/mathcas.xml" "$prefix/share/mime/packages/mathcas.xml"
for size in "${sizes[@]}"; do
  mkdir -p "$prefix/share/icons/hicolor/${size}x$size/apps"
  cp "$app/share/icons/hicolor/${size}x$size/apps/mathcas.png" "$prefix/share/icons/hicolor/${size}x$size/apps/"
done
refresh
echo "MathCAS v2 installed in $app"
case ":$PATH:" in
  *":$prefix/bin:"*) ;;
  *) echo "note: add $prefix/bin to PATH to run mc-repl, mc-notebook and the other programs from a terminal" ;;
esac
