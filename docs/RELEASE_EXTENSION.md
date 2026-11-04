# Release 1.7.2–1.19.2 extension

**0.8.0-dev** enables Netty Release servers from **1.7.2 through 26.3** on
Horizon **26.3 / protocol 777**. The existing launcher instance, displayed as
**ViaFabricPlus 0.8.2**, now runs the verified production binaries from
`dist/play-0.8.2/`, including the [interaction fixes](INTERACTION_FIXES.md)
and [chunk-streaming frame-pacing fix](PERFORMANCE.md).
Its ID remains `viafabricplus-0-7-0`. The 0.8.0 binaries remain under
`dist/play/`, and 0.7.2-dev is backed up under `dist/play/previous-0.7.2-dev/`.

252 Rust tests passed, including 46 standalone tests, 202 transformed
protocol and transport tests, and four signing tests. Live runs use private
Horizon builds and loopback vanilla servers. Java runs the test servers.

## Source work added

- Thirty-five explicit Netty Release protocol descriptors, covering 1.7.2
  through 1.19.2 and terminating at the existing 1.19.3 boundary. They contain
  source/target packet names and step-specific registry maps. Descriptor
  coverage is not gameplay support.
- A Rust connection-owned chain before the established modern translator.
  Incoming packets normalize IDs and structures to 1.19.3; outgoing packets
  cross the same edges in reverse. Login retains the original target format.
- Source adapters connected from **1.14 through 1.19.2**, including villager
  metadata insertion, separated spawn metadata, living-entity metadata changes,
  old join/respawn fields, light/particle coordinates and the 1.16 dimension
  registry. The 1.16.1-to-1.16.2 boundary adds its hardcore flag, maximum-player
  field, recipe-book split and piglin metadata changes. Later edges cover 1.16.4/1.16.5,
  1.17, 1.17.1, 1.18/1.18.1, 1.18.2, 1.19 and 1.19.1/1.19.2.
- Older login hello/profile-key fields, encrypted-nonce response branches,
  1.7 short-length RSA fields, string UUIDs and absent profile properties.
  The host continues to own authentication, encryption and packet compression.
- Separate 1.19.0 and 1.19.1 RSA chat payloads, the five-sender acknowledgement
  window, preceding-signature links, filtering and old chat decoration.
  Certificate V1 and V2 signatures remain distinct; V1 is optional in the
  provider and required when a signed 1.19.0 login uses it.
- Flat and pre-flattening item-stack codecs, compressed 1.7 item NBT,
  old packed positions, spanning/padded bit arrays and checked palettes.
- Chunk bodies from 1.8 through 1.17, inline light separation at 1.14,
  2D-to-3D biomes, 1.16 heightmap packing and partial-column merging at 1.17.
  Real air-state IDs are exported and used for non-air counts.
- The 1.17-to-1.18 combined chunk/light conversion, bounded per-connection
  light caching, old block-entity action IDs and spawner data normalization.
- The 1.16.4-to-1.17 inventory transaction bridge: authoritative slots,
  pending predicted changes, action IDs, rejected-click resynchronization and
  transaction acknowledgements represented by connection-owned ping IDs.
- The 1.15.2-to-1.16 boundary also normalizes legacy dimension identities,
  attribute names, equipment lists, menu/enchantment IDs, chat sender fields,
  interaction flags and player ability speeds. Unsupported jigsaw actions are
  cancelled, as in the upstream forward translator.
- Registry-aware item, recipe, metadata, particle, tag, advancement, statistic,
  block, sound and command-tree conversions for the connected newer edges.
  New-only creative item IDs are cancelled when an old registry lacks them.
- Earlier patch-family conversions for dimension width, keepalives, sign
  packets, trade fields, item wire formats and book hand/slot changes.
- Runtime adapters now route through every Release boundary down to 1.7.2 in
  source. The 1.8-to-1.9 edge uses generated metadata, entity, potion, spawn-egg,
  sound and level-event tables. It translates spawns, movement, passengers,
  inventory slots, held-item interactions and bulk chunks.
- The 1.9.4-to-1.10, 1.10-to-1.11 and 1.11.1-to-1.12 edges translate sound
  IDs and pitch, the resource-pack hash, mob variants and metadata, spawn-egg
  and block-entity identities, menu items and colored beds.
- The 1.12.2-to-1.13 edge remaps block states, entities, particles, items, sound,
  map icons and plugin channels. It includes generated vanilla recipes and
  registry tags, the legacy command suggestion exchange, category-specific
  statistic conversion, and stateful bed, banner, skull and flower-pot block
  updates from NBT.
- The 1.12.2 flattening edge caches the source block states for active chunks.
  It reconstructs fences, panes, walls, stairs, doors, chests, redstone,
  tripwire, chorus plants, fire, snowy ground, double plants and stems, then
  reconciles changes across chunk and section boundaries. The generated
  descriptors use ViaVersion's 1.13 block-state and attachment tables.
- The 1.13-to-1.13.1 and 1.13.2-to-1.14 edges translate entity metadata,
  registries, command trees, menus, trade and book packets, and packed
  positions. The 1.14 edge adds difficulty, chunk-center and view-radius packets.
- The 1.7.2/1.7.6 adapter handles decompression and bulk columns, entity
  spawns and metadata, tab-list entries from server data, map and particle
  packets, menu titles and slots, trade items, and the enchanting lapis slot.
  It also converts grouped block changes, invalid legacy block variants,
  old item identities, book pages and named skull owners. Its sparse
  portal cache reconciles axes across adjacent and bulk chunks and after
  block changes, explosions and unloads.
- Connection-scoped Horizon hooks start and release old sword blocking,
  resolve absent 1.7 skin properties with Horizon's existing profile
  resolver, and fill legacy merchant payment slots with normal inventory
  clicks. The 1.8 adapter publishes the local player's old attack speed.
  The native client applies the old sword-blocking pose with Horizon's
  model scale and follows old boats from server movement updates.
- The 1.16.4 inventory transaction bridge supplies the selected stack to
  1.7/1.8 interactions, including a slot changed by an in-flight click.
  Rejected clicks fall back to server inventory. The client suppresses
  clicks on virtual offhand, brewing fuel and 1.7 lapis slots before
  prediction. The 1.7 adapter refreshes its lapis placeholder when the
  server updates the enchanting input.
- The 1.9.2 edge supplies chunk block-entity placeholders, and the
  flattening edge translates enchanting clue IDs and falls back to plains
  for invalid biome IDs.

## Validation and open compatibility cases

The installed launcher copies were upgraded from 0.7.2-dev to 0.8.0-dev after
the old instance rejected 1.7.2. Both binary hashes match the production receipt.
A fresh Auto world-entry check (`../target/vplay-1.7.2-9a63b79c/result.json`; local artifact)
using `dist/play/` passed against vanilla 1.7.2, loading 81 chunks with zero
decode errors. The upgrade report (`../target/launcher-upgrade-0.8.0.json`; local artifact)
records the wrapper path and retained instance ID.

The isolated probe exercised the production connection entry point with the
new routes enabled before the production bundle admitted them. The live
checks cover world entry, chunks, movement, chat, inventory, block interactions,
mob combat, respawn, portal travel and reconnect.

- All 35 extension protocol families reached the play phase in a replay of
  captured vanilla traffic. The corrected build passed 36 traces, including
  two distinct 1.7.2 captures.
- Live 1.19.2 runs with manual targeting (`../target/vplay-1.19.2-dd9e2ea8/result.json`; local artifact)
  and Auto (`../target/vplay-1.19.2-2c410008/result.json`; local artifact) passed all twelve
  gameplay checks with zero native decode errors.
- The first live 1.7.2 run spawned, rendered chunks, moved, and exchanged chat.
  A dropped-item spawn then exposed a header lost at the 1.10 boundary. The
  fix preserves the header and has passed replay of that actual live trace.
- The next live 1.7.2 run also placed and broke blocks, crafted, opened and
  transferred items from a chest, fought a mob, and respawned. Building a
  portal exposed the 1.16.1-to-1.16.2 grouped block update conversion. The
  adapter now splits updates by chunk section. The 1.14.4 and 1.15.2 edges
  remap grouped updates through their block-state registries.
- The corrected 1.7.2 (`../target/vplay-1.7.2-291e99c6/result.json`; local artifact) and
  1.12.2 (`../target/vplay-1.12.2-c5dda93a/result.json`; local artifact) runs passed all twelve
  live checks. Each entered a Nether portal and retained its server-side
  inventory changes after reconnect.
- The production 1.7.2 run (`../target/vplay-1.7.2-7a3ed2e6/result.json`; local artifact)
  passed the same twelve checks with zero native decode errors. Its binaries
  came from the production build receipt (`../target/native-game-build.json`; local artifact).
  The source inside the [0.8.0-dev package](../dist/viafabricplus-0.8.0-dev.hnmod)
  matches that receipt.
- The production 1.16.5 run (`../target/vplay-1.16.5-9197e732/result.json`; local artifact)
  passed all twelve gameplay checks. Auto also entered and rendered a
  normal-terrain 1.8.9 world (`../target/vplay-1.8.9-6c16c63a/result.json`; local artifact).
- The 37-run production matrix (`../target/vanilla-matrix-cfaad4a5/results.json`; local artifact)
  passed world entry and chunk decoding for the other 34 extension protocol
  families and regression targets 1.19.3, 1.21.1 and 26.3. Together with
  the separate production runs, all 35 new families passed live world entry.

Server-specific profile spoofing, custom plugin channels, and legacy statistic
variants with multiple modern equivalents remain compatibility cases to audit.
Signed command arguments, long-session certificate refresh, and source-version
physics parity remain broader work as recorded in the main port ledger.

## Source locations and provenance

Runtime chain and descriptors: `release_chain.rs`, `release_chain_data.rs`.
Common codecs: `release_codec.rs`, `release_columns.rs`, `release_world.rs`,
`release_inventory.rs`, `release_rewrite.rs`. Stateful edges:
`release_older.rs`, `release_pre18.rs`, `release_113.rs`, `release_flatten.rs`,
`release_110.rs`, `release_18.rs` and `release_17.rs`. Block shapes:
`release_connections.rs` and `release_connection_data.rs`. Metadata and legacy text:
`release_metadata.rs` and `release_text.rs`. Old signed chat: `release_chat.rs`. Integration: `session.rs`, `core.rs`, `horizon-mixin.toml`.

`tools/export-release-chain.py` requires the pinned ViaVersion and ViaLegacy
checkouts, reads their static sources/NBT without a JVM and writes only the
extension's generated Rust data. Sparse pre-flattening item keys are represented
as mapping runs, not dense item indices. Identity maps are generated only for
matching identifier tables or unchanged block-state domains at patch edges.
It does not overwrite the existing modern mapping files or run validation.

ViaVersion: `d8415e6f5991f0b4325207a0477c80e9e3ee1b8f`.
ViaLegacy: `3f7ecf67e74a16aa9b3a0ea9c0b116d072e73618`.
Input checksums are recorded in the generated files; licensing is described in
`THIRD_PARTY_NOTICES.md`. No Java runtime is added to the mod or client.

Artifact paths under `target/` refer to local validation output. They are not
included in this public repository because test runs may contain player or
server metadata. The committed reports in [validation/0.8.2](validation/0.8.2/)
provide the public measurements and build hashes for the current release.
