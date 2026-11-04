> Baseline report for 0.7.0-dev. The current 0.7.1-dev correction and focused
> verification are documented in [FIXES-0.7.1.md](FIXES-0.7.1.md).

# Validation — 1 November 2026

These results apply to **0.7.0-dev**, the Rust native mod for **Horizon 26.3 /
protocol 777**. Gameplay is enabled for Release **1.19.3–26.2**, with native
26.3 pass-through. [Live gameplay results](GAMEPLAY_VALIDATION.md) cover every
enabled protocol family. The wider 1.0.0–26.3 rewrite remains incomplete.

| Check | Result |
| --- | --- |
| Standalone Rust tests | 45 passed: 20 protocol/settings, 17 mapping/wire, 4 mining, 4 hash |
| Transformed Horizon protocol unit tests | 110 passed |
| Injected real-TCP transport fixtures | 18 passed |
| Native registry/item/entity/UI/session fixtures | 34 passed |
| Inventory prediction/cache fixtures | 6 passed |
| Configuration-chain fixtures | 5 passed |
| Gameplay/metadata/predicate/text fixtures | 18 passed |
| Modern/legacy transition regressions | 9 passed |
| Combined selected Rust suites | **245 passed; zero failed or ignored** |
| Standalone Clippy, all targets, warnings denied | Passed |
| Transformed protocol Clippy, all targets, warnings denied | Passed |
| Real horizon-mixins apply | Passed: 14 injections, 71 support modules |
| Production native client and ctl, fast profile | Built successfully |
| Optimized standalone diagnostic CLI | Built; `versions` smoke check passed |
| Python source syntax | All 8 tools passed |
| Rustfmt | 14 changed Rust files passed |
| Live vanilla matrix | **17/17 releases, 204/204 gameplay groups passed** |
| Auto detection and generated terrain, 1.19.3 | Passed |
| Live server transfer, 26.2 → 1.19.3 | Passed |
| Source archive | Pins, all file bytes, CRCs, source/binary receipts verified |
| Repeated packaging | Byte-identical |
| Original Horizon checkout | No tracked modifications |
| Strict full-client Clippy | Four style findings in unchanged host functions |

The regression suite covers dimension-bounded old chunk padding and fluid
counts across fourteen mapping families, dimension selection/reset, legacy
decorated-pot recipes, public gameplay admission over encrypted/compressed
sockets, and transfer handshake adaptation through the real connection hook.
Existing checks cover bidirectional sessions, inventory values and predictions,
registry order, saved text, metadata, split-reader threads, integrated-server
exclusion and byte-preserving native/disabled paths.

The private vanilla transcript replay test is intentionally ignored by ordinary
Cargo runs and invoked explicitly by `tools/replay-vanilla.py`. It is not
included in the 245-test total.

## Artifact and exact build

Artifact: `dist/viafabricplus-0.7.0-dev.hnmod` — **553,408 bytes;
87 files**. SHA-256:
`9de11a96c4164c5ba02d6cb768cbdfecd2ca623c0e33af18094ed65673f49616`.

The source archive includes Rust, TOML and JSON, including the GPL license.
It contains no Java runtime/bytecode, official server jars, prepared Minecraft
resources or precompiled game. The unpacked source bundle is also available
at `dist/viafabricplus-0.7.0-dev/` for launcher native-build.

Horizon revision: `d5f484c89f6e88cee27ad104860587a94b379cd5`.
horizon-mixins revision: `3f99a5c60dfa311cc365524c44ae2fd799bfa1de`.
Bundle digest: `457f0003f61d0ce4538c9a8035b81b3b116217318a5a09e0d384f4e5476ae3b4`.
The build uses `target/horizon-stage`, never the pristine upstream tree.

- `target/horizon-stage/target/fast/horizon-client.exe`: 94,694,793 bytes; SHA-256 `fbe7a4124a4f33f35e24107266af0f6b98ced38a31455b1bfd419b6e9daab844`.
- `target/horizon-stage/target/fast/horizon-ctl.exe`: 76,976,754 bytes; SHA-256 `62d8adfd3217c869b348986209bc3025507e2b6313169cf89dacc1613f0241e8`.

Rust **1.99.1**, `x86_64-pc-windows-gnu`, used workspace-local toolchains
with `RUSTFLAGS=-C link-self-contained=yes`. The bundled linker avoids the
portable GCC installation's missing `libgcc_eh`. Prepared 26.3 resources
were read from the installed `nightly-2026-10-30-1da354b`; that launcher
release remains separate from this source-pinned production build.

## Remaining strict-client lint findings

Full-client/ctl Clippy with warnings denied reports one `if_same_then_else`
and three `collapsible_if` findings in unchanged Horizon functions
(`game/remote.rs` and `game.rs` after transformation). They were
neither suppressed nor patched into the upstream checkout. Standalone and
transformed-protocol strict Clippy both pass. These host style findings do not
prevent compilation or gameplay tests.

## Evidence and limits

Machine-readable summary (`../target/validation-summary-0.7.json`; local artifact),
package verification (`../target/package-validation-0.7.json`; local artifact),
production build receipt (`../target/native-game-build-0.7.json`; local artifact), and
gameplay matrix (`../target/vanilla-matrix-0fd793d3/results.json`; local artifact).

Logs in `target/`: `rust-tests-0.7.log`, `rust-clippy-0.7.log`,
`transport-tests-0.7.log`, `protocol-clippy-0.7.log`,
`client-clippy-0.7.log`, `standalone-build-0.7.log`,
`versions-0.7.log`, `native-build-0.7-final.log`,
`vanilla-production-final-matrix.log`, `vanilla-transfer-final-corrected.log`
and `vanilla-natural-final.log`.

Java was used only for the authorized isolated vanilla test servers. Live tests
used offline mode and compression; encryption plus compression was independently
covered by real TCP fixtures. Online-mode authentication, enforced signed chat,
plugins/custom data packs, and full ViaFabricPlus behavior parity are not proven.
Earlier protocols remain pending. See [PORTING.md](PORTING.md). Historical
0.6 results remain in [VALIDATION-0.6.md](VALIDATION-0.6.md).

Artifact paths under `target/` refer to local validation output. They are not
included in this public repository because test runs may contain player or
server metadata. The committed reports in [validation/0.8.2](validation/0.8.2/)
provide the public measurements and build hashes for the current release.
