# Validation — 31 October 2026

These results apply to **0.6.0-dev**, built for Horizon **26.3 / protocol 777**.
The user explicitly requested builds and tests after the earlier implementation-only work.
The full Release 1.0.0–26.3 port remains incomplete.

| Check | Result |
| --- | --- |
| Standalone Rust tests | 40 passed: 18 protocol/settings, 14 mapping/wire, 4 mining, 4 hash |
| Transformed Horizon protocol unit tests | 110 passed |
| Real injected transport fixtures | 16 passed |
| Native registry/item/entity/UI/session fixtures | 34 passed |
| Inventory prediction/cache fixtures | 6 passed |
| 26.1→26.2→26.3 configuration fixtures | 5 passed |
| Composed gameplay/metadata/predicate/text fixtures | 18 passed |
| Modern and legacy transition regressions | 8 passed |
| Standalone Clippy, all targets, warnings denied | Passed |
| Transformed protocol Clippy, library and tests, warnings denied | Passed |
| Real horizon-mixins apply | Passed: 14 injections, 71 support modules |
| Transformed Horizon client `cargo check --locked` | Passed without warnings |
| Standalone optimized Windows diagnostic CLI | Built; `versions` smoke check passed |
| Python tools | All four syntax checked |
| Source package | Revision pins, manifest, full file contents and CRCs verified |
| Repeated packaging | Byte-identical; package source digest matches the transformed stage |
| Full client/ctl Clippy, warnings denied | Four style findings in unchanged host functions; see below |
| Native client and horizon-ctl executables, fast profile | Passed, including the final source refresh |
| Client and ctl `--help` smoke checks | Passed; client identifies Minecraft 26.3 |
| Isolated game runtime smoke test | Passed: title screen, Vulkan, native mod registration, settings persistence/reload and shutdown |

The combined total is **237 passing tests**: 40 standalone tests, 87 mod
integration tests and 110 Horizon protocol tests. No ignored or failed tests.
The integration harness runs every selected test binary even if one fails.

The eight new regressions verify:

- Login, configuration acknowledgement, queued world-entry packets and keepalives
  in both directions for 16 protocol families from 1.19.3 through 26.2.
- Legacy item names, lore, enchantments and arbitrary NBT surviving the native
  slot writer, then restoring the exact original wire bytes for five families.
- Full changed/carried stack predictions preserving distinct original NBT in
  two stacks of the same item; duplicate-slot normalization and trailing-data rejection.
- Old sign text passing once through Horizon's saved-data fixer and the full session chain.
- Sparse legacy registry numeric IDs surviving configuration expansion.
- Dismount packet bundles, self-teleport angle conversion and locally consumed
  synthetic teleport acknowledgements.
- Pre-1.20.5 encryption requests acquiring the native authentication flag.

Existing fixtures exercise actual local TCP sockets with AES/CFB8 and compression
combined, split-reader thread moves, native/disabled byte preservation,
integrated-server exclusion, configuration preview boundaries, chunk palettes,
block entities, inventory restoration, recipes, metadata, chat and text.
These simulated exchanges do not establish complete server compatibility.

Strict full-client Clippy still reports one `if_same_then_else` and three
`collapsible_if` findings in unchanged Horizon functions after transformation.
The mod drop-hook finding was fixed; the remaining four were not suppressed
or patched into the upstream checkout. These style findings do not prevent
compilation or test execution.

Build fixes in this validation pass:

- Corrected `UPDATE_TAGS` result propagation across source-version branches.
- Kept the receive path's unchanged payload as a borrowed `Cow<[u8]>`.
- Removed three unused item wrappers and corrected three mod Clippy findings.
- Updated older fixtures to check the intended block-entity type ID and conservative
  unknown-hash prediction fallback. Assertions still verify complete values/headers.

Artifact: `dist/viafabricplus-0.6.0-dev.hnmod` — **552,079 bytes; 87 files**.
SHA-256: `b8982022b7902e7748f6acc04af586f211b0c530547d1fa863d4bd55677065b7`.
This source archive contains Rust, TOML and JSON, including the GPL license.
It contains no Java runtime, bytecode, Minecraft resources or precompiled game.

Horizon source: `d5f484c89f6e88cee27ad104860587a94b379cd5`.
Horizon mixins: `3f99a5c60dfa311cc365524c44ae2fd799bfa1de`.
The original Horizon checkout remains unmodified; transformation and compilation
use `target/horizon-stage`. Rust: **1.99.1**, `x86_64-pc-windows-gnu`, with the
workspace-local toolchain and `RUSTFLAGS=-C link-self-contained=yes`.
The bundled Rust linker avoids the portable GCC installation's missing `libgcc_eh`.

Horizon MCP checked installed versions and Rust status. The selected launcher
build is `nightly-2026-10-30-1da354b`; it is separate from this source-pinned build.
The runtime smoke test read that installation's prepared 26.3 resources, with
fresh game, launcher, XDG, trash, clipboard and desktop paths under
`target/smoke-34935fc4`. No Java was run during these builds or tests.

The final executable rendered **3,323 frames** and its smoke report recorded
`error: null`. The Mods page listed **ViaFabricPlus Native 0.6.0-dev (26.3): ON**.
The settings page defaulted to **Native 26.3**, with configuration preview off.
The preview toggle was switched on and off; reloading the saved JSON retained
Native 26.3 and preview off. `horizon-ctl quit` acknowledged shutdown and the
process stopped. No crash report was created. This was a title/settings test;
no world or multiplayer server was opened.

Runtime evidence: settings screenshot (`../target/smoke-34935fc4/settings.png`; local artifact),
game report (`../target/smoke-34935fc4/game-report.json`; local artifact), and
runtime assertions (`../target/smoke-34935fc4/runtime-validation.json`; local artifact).

Built executables:

- `target/horizon-stage/target/fast/horizon-client.exe` — 94,680,549 bytes;
  SHA-256 `b9bcdec4f8420288de454dd55870a4ef6f6e07fbdfcbcfb6b6edea55706fe5ff`.
- `target/horizon-stage/target/fast/horizon-ctl.exe` — 76,905,099 bytes;
  SHA-256 `6d10b3f51e40618d8a84f20ae70a05d3869a535b6ce15202ae4843d3acae3bfa`.

No real older-version multiplayer server or Microsoft-account authentication
was tested. Production older-server connections still stop before acknowledging
configuration. The internal fixture constructors do not promote those versions
to playable capabilities. Full translation audits, older Release families,
version-dependent behavior fixes, complete transformed-value hashes and actual
server gameplay validation remain listed in [PORTING.md](PORTING.md).

Logs and machine-readable package verification are under `target/`:
`rust-tests-0.6.log`, `rust-clippy-0.6.log`, `native-check-0.6.log`,
`transport-tests-0.6.log`, `protocol-clippy-0.6.log`, `client-clippy-0.6.log`,
`standalone-build-0.6.log`, `native-build-0.6.log`, `native-build-final-0.6.log`,
and `package-validation-0.6.json`.
The complete machine-readable result is `target/validation-summary-0.6.json`.

Artifact paths under `target/` refer to local validation output. They are not
included in this public repository because test runs may contain player or
server metadata. The committed reports in [validation/0.8.2](validation/0.8.2/)
provide the public measurements and build hashes for the current release.
