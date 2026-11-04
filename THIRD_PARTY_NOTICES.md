# Third-party notices

- [ViaLegacy](https://github.com/ViaVersion/ViaLegacy), copyright 2020–2026
  RK_01/RaphiMC and contributors, GPL-3.0-or-later. The 0.8 source extension
  derives Release 1.7.2/1.7.6 packet tables, login encodings, compressed chunk
  arrays, item NBT formats and entity metadata layouts from commit
  `3f7ecf67e74a16aa9b3a0ea9c0b116d072e73618`. No pre-Netty transport, Java
  classes, runtime or executable code from that project is bundled.

  The same extension derives 1.8–1.19.3 protocol descriptors, registry maps,
  identifier tables, air-state IDs, dimension/chat registries, motion-blocking
  state ranges, legacy entity and sound tables, and 1.13 recipes, tags, block
  states and connection profiles from the ViaVersion commit
  recorded below. `tools/export-release-chain.py` reads pinned Java source and
  static NBT as data without executing Java. Its generated Rust files record
  SHA-256 hashes of the inputs. Signed chat before 1.19.3 follows
  `ChatSession1_19_0`, `ChatSession1_19_1`, `MessageHeader`, `MessageBody` and
  `ReceivedMessagesStorage` from that same checkout. The source-only status and
  remaining conversion work are recorded in `docs/RELEASE_EXTENSION.md`.

- [ViaFabricPlus](https://github.com/ViaVersion/ViaFabricPlus), ViaVersion and

  contributors, GPL-3.0-or-later. This project is an unofficial Rust port of the

  Release-oriented client integration, currently incomplete. Its name and

  behaviour goals originate there; Fabric/Java classes are not bundled.

- [ViaVersion](https://github.com/ViaVersion/ViaVersion), copyright 2016–2026

  ViaVersion and contributors, GPL-3.0-or-later. Release protocol identifiers,

  ranges, and the 26.1-to-26.2 session UUID change were checked against commit

  `d8415e6f5991f0b4325207a0477c80e9e3ee1b8f`.

  Version 0.2.0 also derives the 26.2-to-26.3 registry mappings and packet-name

  tables from that checkout. The mapping resource's SHA-256 is

  `57da069bb713da20e416a7a50779deeca5b3273a0788036d172ed9f9fc760cd5`.

  These GPL data are embedded as Rust constants, not loaded from Java at runtime.

  The decorated-pot side order and 26.2 block-entity conversion follow that same

  revision's BlockPacketRewriter26_3.

  Registry, item, entity, recipe and text schema conversions follow the same

  revision's v26_2to26_3 rewriters. Block-transformer registry data, brewing-item

  lists and the brick fallback identifier are exported by tools/export-mappings.py.

  The complete block-transformer NBT resource is embedded in registry_data.rs

  as Rust bytes with its own SHA-256, and parsed without a JVM.
  The 0.6.0 development source also embeds the pinned `enchantments-1.21.nbt`
  and `jukebox-songs-1.21.nbt` resources in `registry_1205.rs`, with their SHA-256
  hashes in its generated header. Legacy painting order and attribute modifier
  identities follow `Paintings1_20_5` and `AttributeModifierMappings1_21` from
  the same checkout. They are data used by Rust codecs; no Java is executed.
  The in-progress 1.20.3 adapter also derives NBT item conversions and combined
  registry handling from `BlockItemPacketRewriter1_20_5`,
  `StructuredDataConverter`, and `EntityPacketRewriter1_20_5`. Its bundled
  `damage-types-1.20.3.nbt` resource is recorded with its hash in
  `registry_1203.rs`.

  The 1.20.3 packet/chat state layer also follows `Protocol1_20_3To1_20_5`
  and `AcknowledgedMessagesStorage` at the same revision.

  The 1.19.4–1.20.2 layers follow `Protocol1_19_4To1_20`,
  `Protocol1_20To1_20_2`, `Protocol1_20_2To1_20_3` and their rewriters.
  Configuration bridging, resource-pack identities, JSON/NBT boundaries,
  display rotation and named-NBT changes are implemented in Rust.

  The 1.19.3 layer follows `Protocol1_19_3To1_19_4` and its entity/item
  rewriters. The pinned `damage-types-1.19.4.nbt` resource is embedded in
  `registry_1193.rs` with its SHA-256. Full inventory prediction capture follows
  the pinned Horizon `Game::send_clicks` and `Game::remote_throw` operations.

  Version 0.4.0 also exports 26.1-to-26.2 maps from resource SHA-256

  `c4ff6996cff8281879178ba2eb7f693e7c8a3cfa83d82fffa80064d3e035b832`.

  The configuration chain follows RegistryDataRewriter26_2 and the missing

  song/tag entries in Protocol26_1To26_2. Rust HashOps/CRC32C follows the same

  checkout's hashing format and is checked against ItemHashTest golden values.

  Version 0.5.0 composes those two mapping resources for 26.1 gameplay. Entity

  ancestry comes from EntityTypes26_3; structural changes follow the pinned

  EntityPacketRewriter26_2 and Protocol26_1To26_2. Partial predicate wire layouts

  follow DataComponentPredicate and DataComponentMatchers. Nested hover/text

  traversal follows ComponentRewriterBase and NBTComponentRewriter. Generated

  composed maps record both source-resource checksums.

  Subsequent 1.21.11 implementation follows Protocol1_21_11To26_1 and its

  EntityPacketRewriter26_1, BlockItemPacketRewriter26_1 and RegistryDataRewriter26_1.

  The same checkout supplies mappings-1.21.11to26.1.nbt, fluids-26.1.nbt and

  sound-variant-registries-26.1.nbt; generated Rust constants record each

  source checksum. EitherHolderType, SynchronizedRegistryEitherType and

  ItemTemplateType26_1 define the older holder/template wire layouts.

  The 1.21.9/1.21.10 layer follows Protocol1_21_9To1_21_11 and that protocol's

  entity/item/text rewriters. Its mapping resource and timeline-registry NBT

  are exported from the same pinned checkout, with checksums in tables_1219.rs.

  Synthetic registry entry order is retained independently of NBT map ordering.

  The 1.21.5–1.21.8 work follows Protocol1_21_5To1_21_6,

  Protocol1_21_6To1_21_7 and Protocol1_21_7To1_21_9 and their entity, item,

  particle, text and registry rewriters in that same pinned checkout. The

  generated tables_1215.rs, tables_1216.rs and tables_1217.rs include mapping

  checksums, source registry names and packet names derived from enum ordinals.

  AttributeModifiers1_21, Equippable and ResolvableProfile define the older

  item layouts; the synthetic server-links dialog follows EntityPacketRewriter1_21_6.

  Inline chat holders follow ChatType.TYPE and ChatTypeDecoration.TYPE. The

  client formatting adapter follows the pinned Horizon Game::format_chat;

  direct decorations stay owned by their individual translated packets.

  Dialog traversal and text-bearing dynamic-registry fields follow the same

  revision's RegistryDataRewriter and NBTComponentRewriter, including direct

  dialogs inside show-dialog click events. Custom click payloads stay opaque.

  The 1.21.2–1.21.4 work follows Protocol1_21_2To1_21_4 and

  Protocol1_21_4To1_21_5 and their block/item, entity, particle and text rewriters.

  PaletteType1_18/PaletteType1_21_5, EntityDataTypes1_21_2,

  ItemHashStorage1_21_5 and the older trim/equipment/tool types define those

  wire layouts. Generated tables_1212.rs and tables_1214.rs record the source

  mapping checksums. Java code is not run or bundled by the native mod.

- Minecraft release protocol and DataVersion facts are read from Mojang's
  `version.json`. `tools/data/minecraft-data-versions.json` records the release
  manifests, archive provenance and extracted metadata hashes. The manifest
  URLs originate in Horizon MCP's `instance_versions` response. The exporter
  reads these data without starting a Java runtime.

- [ViaLegacy](https://github.com/ViaVersion/ViaLegacy), copyright 2020–2026

  RK_01/RaphiMC and contributors, GPL-3.0-or-later. Release 1.0.0–1.6.4 protocol

  identifiers were checked against its public LegacyProtocolVersion table.

- [Horizon](https://github.com/horizon-rust/horizon), MIT. Horizon is obtained

  separately and retains its license. The mod adds modules and hooks to its

  original source at build time. No Horizon executable or Minecraft resources

  are included in the `.hnmod` bundle.

  Saved block-entity and entity NBT conversion calls the existing Rust
  `horizon_world::datafix` implementation in this pinned host. No Java data
  fixer is copied or executed.

- [horizon-mixins](https://github.com/horizon-rust/horizon-mixins), MIT. Used as a

  separate development/build tool; not vendored into this project.

Rust dependencies retain their own licenses. The complete GPL text accompanying

this project is in LICENSE. Modified source for the native bundle is shipped in

the `.hnmod` itself and in this source tree.

## MIT notices for adapted API/client code

Copyright (C) 2016–2026 ViaVersion and contributors (MIT-licensed API types).

Copyright (c) 2026 Ludovic Marie and the Horizon contributors.

Permission is hereby granted, free of charge, to any person obtaining a copy

of this software and associated documentation files (the "Software"), to deal

in the Software without restriction, including without limitation the rights

to use, copy, modify, merge, publish, distribute, sublicense, and/or sell

copies of the Software, and to permit persons to whom the Software is

furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all

copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR

IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,

FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE

AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER

LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,

OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE

SOFTWARE.
