# Publishing ViaHorizon

## Validate and package

Use Python 3.11+, Rust 1.99.1, Git and a C toolchain. Build horizon-mixins at
the revision in `compatibility.json` and use its executable below. Internal
networking hooks require exact Horizon and horizon-mixins compatibility pins.

```powershell
python tools/native.py check --horizon PATH_TO_PINNED_HORIZON --mixins PATH_TO_PINNED_MIXINS_EXE
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
python tools/transport-proof.py
python tools/native.py package
```

For an additional package check, install Horizon's checker and run it directly:

```powershell
cargo install --git https://github.com/horizon-rust/horizon --rev fdc87aecdbd116bf1e08adcd062fcbaea8c2e2ca --locked cargo-horizon-mod
cargo horizon-mod check dist/viahorizon-0.8.8-dev.1.hnmod --kind native-mod --json
```

The `.hnmod` carries Rust/TOML sources, GPL license text and third-party notices.
No game assets or executables are bundled.

## GitHub release

Update the version in `Cargo.toml`, `Cargo.lock`, `mixins/viahorizon/core.rs`
and `mixins/viahorizon/horizon-mixin.toml`, plus the current package filenames
in the documentation. Record changes and validation in `docs/releases/`.

Commit and push the source, then create a GitHub release at that commit. Attach
the versioned `.hnmod` and its `.hnmod.sha256` file from `dist/`. Mark development
versions as prereleases. Download the uploaded package and verify its checksum.
Published version contents must never be replaced; bump the version for changes.

## Horizon registry

Registry submissions are separate from GitHub releases. Follow
[Horizon's publishing guide](https://playhorizon.dev/docs/publish-a-mod) and
provide listing details directly in the submission form. Each native version
requires maintainer review.
