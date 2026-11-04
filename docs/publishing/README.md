# Publishing ViaHorizon to Horizon

This file records the registry-specific workflow
from [Horizon's guide](https://playhorizon.dev/docs/publish-a-mod).

## Build and prepare

Use Python 3.11+, Rust 1.95+ (validated with 1.99.1 GNU on Windows), Git and a C
toolchain. Build horizon-mixins at the revision in `compatibility.json` and use
its executable below. The mod hooks internal networking functions, so the
package deliberately has no `hooks` compatibility claim.

```powershell
python tools/native.py check --horizon latest --mixins PATH_TO_PINNED_MIXINS_EXE
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
python tools/transport-proof.py
cargo install --git https://github.com/horizon-rust/horizon --rev 6475f62f3c834b7489e1816bf47f47ea5eb2b879 --locked cargo-horizon-mod
python tools/prepare-publish.py --checker PATH_TO_CARGO_HORIZON_MOD_EXE
```

`--horizon latest` resolves the released Nightly and refuses a different commit
from our compatibility pin. Audit upstream changes, update both manifests and
rerun the check before publishing a new compatibility pin. It never silently
claims compatibility with untested revisions.

`prepare-publish.py` requires a current native-check receipt, packages twice and
compares the bytes, runs `cargo horizon-mod check`, checks the registry entry,
and builds the index it would produce. The `.hnmod` carries Rust/TOML sources,
GPL license text and third-party notices. The package is unsigned; signing is
optional and no registry signing key has been configured.

## Submit for review

Generated files are under `dist/`:

- `viahorizon-0.8.5-dev.hnmod` and its `.sha256` file: package and checksum.
- `publishing/submission.json`: page and version fields for the submission form.
- `publishing/viahorizon.toml`: complete candidate registry entry.
- `publishing/package-check.json`, `readiness.json`, and `index-preview.json`:
  checker results and the index preview.

Open [the submission form](https://playhorizon.dev/mods/submit), sign in with
GitHub and select **Native mod**. Upload the `.hnmod`, and use the values in
`submission.json`. The display name is **ViaHorizon**; the manifest and registry
ID is **viahorizon**. Settings from earlier installations migrate automatically.
Owner/author: `hhhhsffd`. License: `GPL-3.0-or-later`. Source:
<https://github.com/hhhhsffd/viahorizon>. No icon or signature is required.

The form submits to `horizon-rust/horizon-mods` for a maintainer to review.
Preparing this bundle does not submit it or make a registry listing live.
Every native version requires review, and a published package must never be
replaced: change the version before changing its contents.

The port credits [ViaFabricPlus](https://github.com/ViaVersion/ViaFabricPlus)
and other upstream authors in `THIRD_PARTY_NOTICES.md`. Public test keys under
`tests/data/` are deliberately generated fixtures, and do not go in the package.
Player data, tokens, logs, Minecraft files and executables are excluded.
