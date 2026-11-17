#!/usr/bin/env bash
# Builds the Flatpak of this checkout as one file: MathCAS-v2-<version>-<commit>-linux-<arch>.flatpak.
#   flatpak install --user MathCAS-v2-<version>-<commit>-linux-x64.flatpak     (the runtime comes from Flathub)
#   flatpak run io.github.rajatmo.MathCASv2
# The manifest of this folder builds a release tag; here its source is this checkout, and the crate sources are
# written from this checkout's Cargo.lock.
# Usage: packaging/flatpak/build-bundle.sh [OUTDIR]       (default OUTDIR: <work dir>/packages)
# Environment: FLATPAK_WORK (the work folder, default ./target/flatpak), MC_COMMIT (when the source is not a git
# checkout). Needs flatpak, flatpak-builder and python3; the runtime, the SDK and the Rust extension are installed
# for this user from Flathub when they are missing. Prints the bundle's path last.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
here="$root/packaging/flatpak"
id=io.github.rajatmo.MathCASv2
work="${FLATPAK_WORK:-$root/target/flatpak}"
out="${1:-$work/packages}"
mkdir -p "$work" "$out"
out="$(cd "$out" && pwd)"

version="$(sed -n 's/^version = "\([^"]*\)".*/\1/p' "$root/Cargo.toml" | head -n 1)"
commit="${MC_COMMIT:-$(git -C "$root" rev-parse --short=7 HEAD 2>/dev/null || echo local)}"
case "$(uname -m)" in
  x86_64) arch=x64 ;;
  aarch64) arch=arm64 ;;
  *) echo "build-bundle.sh: unsupported machine $(uname -m)" >&2; exit 2 ;;
esac
name="MathCAS-v2-$version-$commit-linux-$arch"
echo "[1/4 25%] sources of $name"

# the manifest of this folder with the release tag replaced by this checkout, and the files beside it
cp "$here/$id.desktop" "$here/$id.metainfo.xml" "$here/$id.xml" "$work/"
python3 "$here/gen-cargo-sources.py" "$root/Cargo.lock" "$work/cargo-sources.json"
python3 - "$here/$id.yml" "$work/$id.yml" "$root" "$commit" <<'EOF'
import re
import sys

src, dst, root, commit = sys.argv[1:5]
text = open(src, encoding="utf-8").read()
git = re.compile(r"      - type: git\n        url: [^\n]*\n        tag: [^\n]*\n        commit: [^\n]*\n")
if not git.search(text):
    sys.exit("build-bundle.sh: the manifest's git source was not found")
checkout = f"      - type: dir\n        path: {root}\n        skip:\n          - target\n          - .git\n"
text = git.sub(lambda _: checkout, text)
text = re.sub(r"MC_COMMIT: [^\n]*", f"MC_COMMIT: {commit}", text)
open(dst, "w", encoding="utf-8").write(text)
EOF

echo "[2/4 50%] building (the crates are downloaded once and kept in $work/.flatpak-builder)"
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
(cd "$work" && flatpak-builder --user --force-clean --disable-rofiles-fuse --install-deps-from=flathub \
  --repo=repo build-dir "$id.yml" </dev/null)

echo "[3/4 75%] bundling"
bundle="$out/$name.flatpak"
rm -f "$bundle"
flatpak build-bundle --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo "$work/repo" "$bundle" "$id"

echo "[4/4 100%] done"
du -h "$bundle"
echo "$bundle"
