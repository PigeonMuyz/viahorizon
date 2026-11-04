# Live gameplay validation — 0.7.1-dev, 1 November 2026

**17/17 real vanilla server releases passed all twelve gameplay groups:
204/204 groups, zero packet decode errors at the checked boundaries.**
This run covers every enabled protocol family on the current **0.7.1-dev**
production build, including native 26.3. No additional mod fixes were required.

The client remains **Horizon 26.3 / protocol 777**, with ViaFabricPlus ON,
an explicit server target and configuration preview OFF. These tests use the
normal public gameplay path, with no fixture admission or probe binary.

| Server family | Protocol | Actual release tested | Result | Evidence |
| --- | --- | --- | --- | --- |
| 1.19.3 | 761 | 1.19.3 | 12/12 passed | Result (`../target/vplay-1.19.3-767693c4/result.json`; local artifact) |
| 1.19.4 | 762 | 1.19.4 | 12/12 passed | Result (`../target/vplay-1.19.4-b4310505/result.json`; local artifact) |
| 1.20–1.20.1 | 763 | 1.20.1 | 12/12 passed | Result (`../target/vplay-1.20.1-9c47138a/result.json`; local artifact) |
| 1.20.2 | 764 | 1.20.2 | 12/12 passed | Result (`../target/vplay-1.20.2-3b3f9030/result.json`; local artifact) |
| 1.20.3–1.20.4 | 765 | 1.20.4 | 12/12 passed | Result (`../target/vplay-1.20.4-156d5562/result.json`; local artifact) |
| 1.20.5–1.20.6 | 766 | 1.20.6 | 12/12 passed | Result (`../target/vplay-1.20.6-c1bd76e6/result.json`; local artifact) |
| 1.21–1.21.1 | 767 | 1.21.1 | 12/12 passed | Result (`../target/vplay-1.21.1-ceecb9cc/result.json`; local artifact) |
| 1.21.2–1.21.3 | 768 | 1.21.3 | 12/12 passed | Result (`../target/vplay-1.21.3-dabf72f7/result.json`; local artifact) |
| 1.21.4 | 769 | 1.21.4 | 12/12 passed | Result (`../target/vplay-1.21.4-d80dfb99/result.json`; local artifact) |
| 1.21.5 | 770 | 1.21.5 | 12/12 passed | Result (`../target/vplay-1.21.5-0f8ef807/result.json`; local artifact) |
| 1.21.6 | 771 | 1.21.6 | 12/12 passed | Result (`../target/vplay-1.21.6-19500c52/result.json`; local artifact) |
| 1.21.7–1.21.8 | 772 | 1.21.8 | 12/12 passed | Result (`../target/vplay-1.21.8-bc61aec7/result.json`; local artifact) |
| 1.21.9–1.21.10 | 773 | 1.21.10 | 12/12 passed | Result (`../target/vplay-1.21.10-a2337576/result.json`; local artifact) |
| 1.21.11 | 774 | 1.21.11 | 12/12 passed | Result (`../target/vplay-1.21.11-61f291e7/result.json`; local artifact) |
| 26.1–26.1.2 | 775 | 26.1.2 | 12/12 passed | Result (`../target/vplay-26.1.2-814e7d3d/result.json`; local artifact) |
| 26.2 | 776 | 26.2 | 12/12 passed | Result (`../target/vplay-26.2-1f0613d1/result.json`; local artifact) |
| 26.3 (native) | 777 | 26.3 | 12/12 passed | Result (`../target/vplay-26.3-41e12ceb/result.json`; local artifact) |

One representative release was tested per protocol family. Patch aliases share
an adapter; this is not a claim that every patch binary was tested.

## What every server passed

1. Login, configuration and player spawn into the world.
2. At least nine loaded chunks, native meshing and zero decode errors.
3. Real player movement retained after a subsequent state update.
4. Player chat echoed by the server.
5. Block placement and breaking confirmed by server block queries.
6. Survival inventory clicks confirmed against the server inventory.
7. Crafting a table from four planks, confirmed in server inventory.
8. Receiving chest contents and shift-clicking them into the player inventory.
9. Receiving a husk spawn and attacking it, confirmed by server-side health.
10. Death and a successful live respawn.
11. Nether travel and return to the overworld.
12. Disconnect/reconnect with loaded chunks and zero decode errors.

The driver checks zero decode errors at initial world entry, after gameplay,
and after reconnect. These are twelve scenario groups per release, not 204
independent packet tests. Screenshots and server assertions are in each linked
run folder alongside the result.

## Exact build and isolation

Source digest: `fcbb6036a9eeb97417c5eb096fdc61622f0bfe5d382dd69e67057bd50df669c9`.
Client SHA-256: `b6facb6392498d33562b0963c7b576e99e17fc8bb1d4fe54fd23f0c316103b0f`.
Package SHA-256: `83002f9c3f845f24b4661dbc023fffa086c9d6b261283f6f78d36ee8ca61b0a7`.

Build receipt (`../target/native-game-build-0.7.1.json`; local artifact),
complete matrix (`../target/vanilla-matrix-5d82f37b/results.json`; local artifact), and
machine summary (`../target/validation-summary-0.7.1.json`; local artifact).
The installed prebuilt client's hashes match the validated build.

Server jars and metadata came from checksum-verified Mojang URLs obtained
through Horizon MCP's official release catalog. Servers were localhost-only,
offline mode, with compression threshold 256. Java ran only the authorized
isolated test servers. The mod, client and launcher helper remain Rust.

Each run used fresh worlds, game/launcher homes, XDG, trash, clipboard and
desktop paths under `target/`. Self-update and audio were disabled.
Every result confirms its processes stopped. The upstream Horizon checkout
remains pristine. Official jars, resources and private transcripts are excluded
from the mod archive.

## Other evidence and limits

The [0.7.1 correction report](FIXES-0.7.1.md) records 247 passing Rust tests and
a separate live 1.21.1 test with 186 deliberately modified chunk counters.
The player subsequently confirmed that their online-mode 1.21.1 server works.
That user observation is separate from this automated offline-mode matrix.

The [historical 0.7.0 matrix](GAMEPLAY_VALIDATION-0.7.md) also records transfer,
Auto detection and generated-terrain checks on that preceding build.
This new matrix does not independently retest those extra cases.

These results cover the listed vanilla gameplay scenarios. They do not prove
every public server/plugin/datapack, enforced signed chat, anti-cheat behavior,
every packet/item/entity/GUI combination, or complete ViaFabricPlus behavior
parity. Release versions before 1.19.3 remain unimplemented.
See the [port ledger](PORTING.md).

Artifact paths under `target/` refer to local validation output. They are not
included in this public repository because test runs may contain player or
server metadata. The committed reports in [validation/0.8.2](validation/0.8.2/)
provide the public measurements and build hashes for the current release.
