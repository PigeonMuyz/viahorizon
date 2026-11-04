# Release-only port ledger

The host is Horizon **26.3 / protocol 777**. The installed **0.7.2-dev** enables
Rust translation for the sixteen Release protocol families **761–776**
(1.19.3–26.2), with native 26.3 pass-through. Earlier Release targets are
cataloged but remain rejected. Beta, Alpha, Classic, Bedrock and snapshots are
excluded. The longer-term 1.0.0–26.3 rewrite is not finished.

Working source **0.8.0-dev** adds the pre-1.19.3 conversion chain, with 1.7.2
as the current lower target. It is unfinished, unbuilt and untested; no new
Release has been promoted to gameplay. [Extension progress](RELEASE_EXTENSION.md)
records the source work and remaining boundaries. Builds and tests are on hold
until the user explicitly asks for them.

## Implemented gameplay path

The public constructor admits supported targets without any fixture override.
[Signed chat](SIGNED_CHAT.md) adds certificate retrieval, per-connection signatures
and acknowledgement tracking for translated and native connections.
Configuration preview remains a separate opt-in diagnostic boundary. Per-server
selection, status-based detection, settings persistence, login/configuration
state, queued replies, reconfiguration and gameplay are connected to the real
Horizon transport. Compression, encryption and authentication remain host-owned.

Pre-1.20.2 joins synthesize configuration from combined registries and retain
early play packets until the native acknowledgement. Later registries retain
source ordering and add required native defaults. Static maps are composed from
the original release and applied once; dynamic identifiers remain connection-local.

Items support legacy NBT and modern components, nested stacks, text, books,
profiles, enchantments, modifiers, recipes, menus, trades, and checked restoration
data. Hooks capture full changed/carried stacks before the host hashes them for
older click packets. Bounded prediction caches retain known transformed hashes;
unknown transformations request ordinary server resynchronization.

World adapters translate chunks, palettes, heightmaps, fluid counts, light,
block changes and block entities. Registry dimension heights bound old section
buffers and permit unused zero padding while rejecting nonzero trailing data.
Login/respawn updates the active dimension. Older block-entity NBT passes through
Horizon's Rust saved-data fixer with the source release's DataVersion.

Entity adapters cover spawn/type changes, metadata serializers/indices,
movement and teleports, attributes, equipment, particles, passengers, damage,
death and respawn. Text conversion follows component positions and hover items
without rewriting arbitrary custom item data. Recipes include the legacy
decorated-pot category field. Transfers to pre-1.20.5 servers downgrade the
newer transfer intention to an ordinary login intention.

[Gameplay validation](GAMEPLAY_VALIDATION.md) records actual local vanilla runs
through the normal production path, including server-confirmed inventory,
blocks, crafting, combat, dimensions and reconnects. [Build validation](VALIDATION.md)
records native compilation, standalone checks and encrypted/compressed fixtures.
The historical 0.6 results are retained in [VALIDATION-0.6.md](VALIDATION-0.6.md).

## Remaining scope and limits

| Area | Remaining work |
| --- | --- |
| Online servers | Ordinary signed chat and authenticated joins verified on 1.19.3, 1.21.1 and native 26.3; signed command arguments and broader proxy/plugin cases remain |
| Plugins and custom data packs | Dynamic registry, custom payload, resource-pack and server-extension audits; vanilla tests do not cover these |
| ViaFabricPlus behavior parity | Source-version movement, interaction, collision, visual and sound differences; older pick-item emulation |
| Advanced item values | Complete transformed-value hashes, changed partial-predicate NBT schemas, edited restoration markers, and new-only components |
| UI and entities | Broader menu/dialog, resource-pack, vehicle, mob and inline-holder combinations beyond the normal-play matrix |
| Release 1.7.2–1.19.2 | Additional protocol, registry/content, item and behavioral chains with real server tests |
| Release 1.0.0–1.6.4 | Legacy stream framing, packets, authentication and ping in addition to gameplay conversion |

Unsupported new-only creative items/actions may be cancelled when the target has
no representation. The pinned renderer preserves profile data but may not
display every explicit skin override. Packet codecs intentionally reject
unconverted schemas instead of forwarding incompatible bytes.

These limits mean this is not complete ViaFabricPlus feature parity or a claim
of compatibility with every public server, anti-cheat or plugin. The supported
range describes the enabled gameplay translators and the tested vanilla
protocol families.

## Extending support

Keep the client on 26.3. Add a source-release adapter and its registry maps;
compose it with the existing chain. Reusing a numeric protocol ID is insufficient:
legacy 1.4.2 and Netty 1.8 both use 47 but require separate identities/transports.

Before admitting another family, verify both packet directions and transitions,
real Horizon sockets with compression/encryption, and actual world entry with
authoritative gameplay assertions. Retain native/disabled pass-through and
integrated-server exclusion. Use fresh game/launcher homes for all development
runs; keep official jars, prepared resources and transcripts out of public
source and distributable packages.
