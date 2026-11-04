# Vanilla gameplay validation — 1 November 2026

**All 17 tested releases passed all twelve normal-play check groups: 204 groups.**
Sixteen translated protocol families cover Release **1.19.3–26.2**; 26.3 is the
native regression baseline. The player remains Horizon 26.3 / protocol 777.
The tests used the **normal production 0.7.0-dev build**, with preview off,
without fixture admission or a quarantined gameplay-probe binary.

| Server family | Protocol | Actual vanilla release tested | Result |
| --- | --- | --- | --- |
| 1.19.3 | 761 | 1.19.3 | 12/12 passed |
| 1.19.4 | 762 | 1.19.4 | 12/12 passed |
| 1.20–1.20.1 | 763 | 1.20.1 | 12/12 passed |
| 1.20.2 | 764 | 1.20.2 | 12/12 passed |
| 1.20.3–1.20.4 | 765 | 1.20.4 | 12/12 passed |
| 1.20.5–1.20.6 | 766 | 1.20.6 | 12/12 passed |
| 1.21–1.21.1 | 767 | 1.21.1 | 12/12 passed |
| 1.21.2–1.21.3 | 768 | 1.21.3 | 12/12 passed |
| 1.21.4 | 769 | 1.21.4 | 12/12 passed |
| 1.21.5 | 770 | 1.21.5 | 12/12 passed |
| 1.21.6 | 771 | 1.21.6 | 12/12 passed |
| 1.21.7–1.21.8 | 772 | 1.21.8 | 12/12 passed |
| 1.21.9–1.21.10 | 773 | 1.21.10 | 12/12 passed |
| 1.21.11 | 774 | 1.21.11 | 12/12 passed |
| 26.1–26.1.2 | 775 | 26.1.2 | 12/12 passed |
| 26.2 | 776 | 26.2 | 12/12 passed |
| 26.3 (native) | 777 | 26.3 | 12/12 passed |

One representative release was run per protocol family. Patch aliases share
the protocol adapter; this is not a claim that every patch binary was run.

## What each server passed

1. Player spawn and world entry through login/configuration.
2. At least nine loaded chunks, native meshing, and zero packet decode errors.
3. Actual player movement that remains after a subsequent state update.
4. Player chat echoed by the server.
5. Block placement and breaking, checked with server `execute if block`.
6. Survival inventory clicks, checked against the server's player inventory.
7. Four planks crafted into a crafting table, confirmed in server inventory.
8. Chest contents received and shift-clicked into the player inventory, confirmed by the server.
9. Husk spawn received and an actual attack changing its server-side health.
10. Death screen followed by a live respawn.
11. Nether travel and return to the overworld.
12. Disconnect/reconnect with loaded chunks and zero decode errors.

The driver also checks zero decode errors after gameplay and after reconnect.
These are twelve scenario groups per server, not 204 independent packet tests.

## Additional final-build checks

- **Automatic target selection plus generated terrain on 1.19.3:** passed world
  entry and native chunk decoding with preview off. Result (`../target/vplay-1.19.3-aeac52e2/result.json`; local artifact)
  and screenshot (`../target/vplay-1.19.3-aeac52e2/world.png`; local artifact).
- **Server transfer from 26.2 to 1.19.3 with Auto selected:** passed destination
  world entry, loaded chunks, and zero decode errors. This exercises a real
  transfer packet and downgrades the transfer intention for the older server.
  Result (`../target/vplay-1.19.3-2a2d87a4/result.json`; local artifact).
- Real TCP Rust fixtures independently combine **AES/CFB8 and compression**,
  including the public gameplay constructor and transfer intentions.

## Provenance and isolation

Final source bundle digest:
`457f0003f61d0ce4538c9a8035b81b3b116217318a5a09e0d384f4e5476ae3b4`.

Production build receipt (`../target/native-game-build-0.7.json`; local artifact) records both native
binary hashes. Every live result records the same source digest. The
complete matrix (`../target/vanilla-matrix-0fd793d3/results.json`; local artifact) and
machine summary (`../target/validation-summary-0.7.json`; local artifact) retain the assertions.

Horizon MCP supplied the official Minecraft release manifest. The driver checked
Mojang metadata/server-jar SHA-1 and sizes. Servers were vanilla, localhost-only,
**offline mode**, with compression threshold 256. Java ran only those isolated
test servers, as authorized by the user. The client and mod are Rust.

Fresh worlds, game/launcher homes, XDG paths, trash, clipboard and desktop paths
were created under `target/`. Self-update and audio were disabled. Every run
reports that its client and servers stopped. No launcher instance or upstream
source checkout was modified. Server jars, prepared resources, accounts and
private wire transcripts are excluded from the distributable source archive.

## Per-release evidence

- **1.19.3**: result (`../target/vplay-1.19.3-c6ca47aa/result.json`; local artifact), world screenshot (`../target/vplay-1.19.3-c6ca47aa/world.png`; local artifact).
- **1.19.4**: result (`../target/vplay-1.19.4-1da2a6d0/result.json`; local artifact), world screenshot (`../target/vplay-1.19.4-1da2a6d0/world.png`; local artifact).
- **1.20.1**: result (`../target/vplay-1.20.1-422e77c5/result.json`; local artifact), world screenshot (`../target/vplay-1.20.1-422e77c5/world.png`; local artifact).
- **1.20.2**: result (`../target/vplay-1.20.2-486ee0aa/result.json`; local artifact), world screenshot (`../target/vplay-1.20.2-486ee0aa/world.png`; local artifact).
- **1.20.4**: result (`../target/vplay-1.20.4-3e237514/result.json`; local artifact), world screenshot (`../target/vplay-1.20.4-3e237514/world.png`; local artifact).
- **1.20.6**: result (`../target/vplay-1.20.6-8ddbcf79/result.json`; local artifact), world screenshot (`../target/vplay-1.20.6-8ddbcf79/world.png`; local artifact).
- **1.21.1**: result (`../target/vplay-1.21.1-6ff0b9ad/result.json`; local artifact), world screenshot (`../target/vplay-1.21.1-6ff0b9ad/world.png`; local artifact).
- **1.21.3**: result (`../target/vplay-1.21.3-1d61d44b/result.json`; local artifact), world screenshot (`../target/vplay-1.21.3-1d61d44b/world.png`; local artifact).
- **1.21.4**: result (`../target/vplay-1.21.4-48d2edfc/result.json`; local artifact), world screenshot (`../target/vplay-1.21.4-48d2edfc/world.png`; local artifact).
- **1.21.5**: result (`../target/vplay-1.21.5-7ddd6add/result.json`; local artifact), world screenshot (`../target/vplay-1.21.5-7ddd6add/world.png`; local artifact).
- **1.21.6**: result (`../target/vplay-1.21.6-48c49ec7/result.json`; local artifact), world screenshot (`../target/vplay-1.21.6-48c49ec7/world.png`; local artifact).
- **1.21.8**: result (`../target/vplay-1.21.8-09485efe/result.json`; local artifact), world screenshot (`../target/vplay-1.21.8-09485efe/world.png`; local artifact).
- **1.21.10**: result (`../target/vplay-1.21.10-b77d3787/result.json`; local artifact), world screenshot (`../target/vplay-1.21.10-b77d3787/world.png`; local artifact).
- **1.21.11**: result (`../target/vplay-1.21.11-0949ebd4/result.json`; local artifact), world screenshot (`../target/vplay-1.21.11-0949ebd4/world.png`; local artifact).
- **26.1.2**: result (`../target/vplay-26.1.2-ef8db6f6/result.json`; local artifact), world screenshot (`../target/vplay-26.1.2-ef8db6f6/world.png`; local artifact).
- **26.2**: result (`../target/vplay-26.2-94aa4122/result.json`; local artifact), world screenshot (`../target/vplay-26.2-94aa4122/world.png`; local artifact).
- **26.3**: result (`../target/vplay-26.3-078a6de2/result.json`; local artifact), world screenshot (`../target/vplay-26.3-078a6de2/world.png`; local artifact).

## Practical limits

These tests establish world entry and the listed vanilla gameplay scenarios.
They do not establish Microsoft/online-mode cross-version authentication,
enforced signed chat, custom plugin/data-pack compatibility, every packet/item/
entity/GUI combination, anti-cheat behavior, or complete ViaFabricPlus physics
and visual parity. The pinned host currently sends unsigned chat. Release
versions before 1.19.3 remain unimplemented. See the [port ledger](PORTING.md).

Artifact paths under `target/` refer to local validation output. They are not
included in this public repository because test runs may contain player or
server metadata. The committed reports in [validation/0.8.2](validation/0.8.2/)
provide the public measurements and build hashes for the current release.
