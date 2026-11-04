# 0.7.1-dev chunk-count correction — 1 November 2026

The user reached their online-mode 1.21.1 server after enabling the mod and
launching through the signed-in launcher, then disconnected with
`Invalid old non-air count`. The previous full matrix used offline vanilla
servers and did not exercise unusual advisory counters.

The translator added a `0..=4096` range check to a signed-short chunk field.
ViaVersion's pinned `ChunkSectionType1_18` and `ChunkSectionType26_1`
preserve these fields without that check, as does Horizon's native `Section`
codec. Version 0.7.1 preserves their bytes in all three translation passes.
These counters do not allocate buffers or select palette lengths. Packet,
section, palette, identifier, storage-index and padding validation remain active.

The public server's failing packet/count was not recorded. This fix removes
the check that generated the reported error; it does not establish complete
compatibility with that server before the player retries it.

| Verification on the corrected production build | Result |
| --- | --- |
| Standalone tests | 46 passed |
| Transformed host/mod tests | 201 passed |
| Combined selected Rust tests | **247 passed; zero failures or ignored tests** |
| Standalone strict Clippy | Passed, all targets |
| Transformed protocol strict Clippy | Passed, all targets |
| Real SDK application | 14 injections, 71 modules |
| Native client and ctl | Built successfully |
| 1.21.1 with deliberately unusual counters | **12/12 gameplay groups passed** |
| Ordinary 1.19.3 and native 26.3 | **24/24 gameplay groups passed** |
| Source package | CRCs, source bytes, pins and repeatable packaging verified |
| Deployment | Updated the existing launcher's prebuilt client; account wrapper retained |

The new standalone regression covers signed-short boundary values across all
sixteen translated families. The native decoder regression checks 1.21.1,
1.21.5, 26.1 and 26.2 layouts. Malformed/truncated palette buffers still fail.

`tools/vanilla-advisory-counts.py` changes only the first section's advisory
counter in real localhost vanilla 1.21.1 chunk packets, rotating through
`-32768, -1, 4097, 8192, 32767`. It preserves framing, compression, palette
data and all other fields. Both the initial connection and reconnect exercised
all five values: **186 modified chunk packets**.
The full scenarios include loaded/meshed chunks, movement, chat, server-confirmed
blocks/inventory/crafting/chest transfers/combat, respawn, dimensions and reconnect.
There were zero native decode errors at entry, after gameplay and after reconnect.

Java ran only the authorized isolated vanilla test servers; the mod, client and
launcher helper remain Rust. Fresh homes/worlds were used and all test processes
stopped. The original Horizon source is pristine. Strict full-client Clippy has
the same four previously documented host style findings; this correction does
not touch those functions. The existing [baseline report](VALIDATION.md) records
the preceding full seventeen-release matrix.

Package: `dist/viafabricplus-0.7.1-dev.hnmod`; 553,544 bytes,
87 files. SHA-256:
`83002f9c3f845f24b4661dbc023fffa086c9d6b261283f6f78d36ee8ca61b0a7`.

Source digest: `fcbb6036a9eeb97417c5eb096fdc61622f0bfe5d382dd69e67057bd50df669c9`.
Machine report (`../target/validation-summary-0.7.1.json`; local artifact),
counter-injection result (`../target/vplay-1.21.1-83a01727/result.json`; local artifact),
mutation counts (`../target/vplay-1.21.1-83a01727/advisory-counts.json`; local artifact), and
ordinary-server matrix (`../target/vanilla-matrix-ffea2a26/results.json`; local artifact).

The launcher instance **ViaFabricPlus 0.7.0** keeps its existing name and id but
now runs **0.7.1-dev** from `dist/play/`. Launch it through Horizon Launcher
to receive the account session. The old executables are retained under
`dist/play/previous-0.7.0-dev/`. No game rebuild is needed by the player.

The subsequent [full 0.7.1 live matrix](GAMEPLAY_VALIDATION.md) passed all 17 releases and 204 gameplay groups without further fixes.

Artifact paths under `target/` refer to local validation output. They are not
included in this public repository because test runs may contain player or
server metadata. The committed reports in [validation/0.8.2](validation/0.8.2/)
provide the public measurements and build hashes for the current release.
