# Flatpak

The files of the Flatpak package `io.github.rajatmo.MathCASv2`:

- `io.github.rajatmo.MathCASv2.yml`: the manifest. It builds a release tag from source without network access
  and installs the notebook (the application), the documentation window, the REPL, the kernel and the language
  server into `/app/bin`.
- `cargo-sources.json`: every crate of that tag's `Cargo.lock` as a source, written by `gen-cargo-sources.py`.
- `io.github.rajatmo.MathCASv2.desktop`, `.metainfo.xml`, `.xml` (the MIME types of `.mcn` and `.mc`).

For a new release: change `tag`, `commit` and `MC_COMMIT` in the manifest, add a `<release>` to the metainfo
file, and write the crate sources again from the tag's lock file:

    git show v<version>:Cargo.lock > /tmp/Cargo.lock
    python3 packaging/flatpak/gen-cargo-sources.py /tmp/Cargo.lock

To build and try it:

    flatpak install flathub org.freedesktop.Platform//25.08 org.freedesktop.Sdk//25.08 \
        org.freedesktop.Sdk.Extension.rust-stable//25.08
    cd packaging/flatpak
    flatpak-builder --user --install --force-clean build-dir io.github.rajatmo.MathCASv2.yml
    flatpak run io.github.rajatmo.MathCASv2

To check the metadata: `appstreamcli validate io.github.rajatmo.MathCASv2.metainfo.xml` and
`desktop-file-validate io.github.rajatmo.MathCASv2.desktop`.
