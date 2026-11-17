#!/usr/bin/env python3
"""Writes cargo-sources.json for the Flatpak manifest from the workspace's Cargo.lock.

Flatpak builds have no network access: every crate of the lock file is listed as a source (its archive from
crates.io with the lock file's checksum), unpacked into cargo/vendor, and a cargo configuration replaces the
registry with that folder.
    python3 packaging/flatpak/gen-cargo-sources.py [Cargo.lock] [cargo-sources.json]
"""
import json
import sys
import tomllib
from pathlib import Path

here = Path(__file__).resolve().parent
lock = Path(sys.argv[1]) if len(sys.argv) > 1 else here.parent.parent / "Cargo.lock"
out = Path(sys.argv[2]) if len(sys.argv) > 2 else here / "cargo-sources.json"
REGISTRY = "registry+https://github.com/rust-lang/crates.io-index"
VENDOR = "cargo/vendor"

sources = []
for p in tomllib.loads(lock.read_text(encoding="utf-8"))["package"]:
    src = p.get("source")
    if src is None:
        continue  # a crate of the workspace
    if src != REGISTRY:
        sys.exit(f"{p['name']} {p['version']}: source {src} is not crates.io; add its handling here")
    name, version, checksum = p["name"], p["version"], p["checksum"]
    dest = f"{VENDOR}/{name}-{version}"
    sources.append({
        "type": "archive",
        "archive-type": "tar-gzip",
        "url": f"https://static.crates.io/crates/{name}/{name}-{version}.crate",
        "sha256": checksum,
        "dest": dest,
    })
    sources.append({
        "type": "inline",
        "contents": json.dumps({"package": checksum, "files": {}}),
        "dest": dest,
        "dest-filename": ".cargo-checksum.json",
    })
sources.append({
    "type": "inline",
    "contents": (
        '[source.crates-io]\nreplace-with = "vendored-sources"\n\n'
        f'[source.vendored-sources]\ndirectory = "{VENDOR}"\n'
    ),
    "dest": "cargo",
    "dest-filename": "config.toml",
})
out.write_text(json.dumps(sources, indent=2) + "\n", encoding="utf-8")
print(f"{out}: {(len(sources) - 1) // 2} crates")
