# 0.8.1 interaction fixes

Two interaction bugs affected older Release servers on Horizon 26.3.

Right-clicking an item in the air on 1.7.2/1.7.10 disconnected the client.
The 1.8 adapter correctly generated the no-target position `(-1, -1, -1)`
and face `255`, but the 1.7 adapter tried to encode its Y as an ordinary
block height. It rejected `-1` as outside the unsigned-byte range. The fix
encodes that specific sentinel as Y `255`, retaining validation for actual
block positions. Held items and cursor bytes still pass through their codecs.

Block placement on servers before 1.19 could briefly revert to air, then
reappear when the server replied. The translator generated a local sequence
acknowledgement immediately, while Horizon still retained the pre-click state.
That acknowledgement restored the old block before the server had processed
the action. Connection-scoped hooks now use the earlier vanilla behavior:
keep the optimistic local block, settle sequence bookkeeping without rolling
it back, and apply authoritative server block updates immediately. Servers
from 1.19 onward retain Horizon's normal sequence-based prediction behavior.
The check uses the connected session, so changing settings cannot alter an
existing connection or affect an integrated single-player world.

The 0.8.0 baseline (`../target/vplay-1.7.2-7b164c6b/result.json`; local artifact) reproduces
the air right-click disconnect against vanilla 1.7.2. A packet regression
also failed on the original source with `Legacy Y exceeds byte`.

All 250 Rust tests passed: 46 standalone tests and 204 host/transport tests,
including both new packet regressions. The installed 0.8.1 binaries passed
all 15 live 1.7.2 checks (`../target/vplay-1.7.2-f98b84b6/result.json`; local artifact),
including the three interaction regressions. All five
samples taken before forwarding the placement (`../target/vplay-1.7.2-f98b84b6/placement-before-server.json`; local artifact)
remained stone. World entry, gameplay and reconnect had zero decode errors.
The same installed binaries also passed all twelve
1.19.2 Auto gameplay checks (`../target/vplay-1.19.2-c31d8d7a/result.json`; local artifact).
Build/package hashes, launcher routing and results are collected in the
0.8.1 validation report (`../target/validation-summary-0.8.1.json`; local artifact).

The existing instance is now named **ViaFabricPlus 0.8.1** and routes through
`dist/play-0.8.1/viafabricplus-launch.exe`. The old binaries remain available;
restart both the launcher and game to use the new wrapper path. Instance
settings and saved servers retain the same `viafabricplus-0-7-0` ID.

Regression coverage includes the complete 1.19.3-to-1.7/1.8 packet chain,
held-item and height-boundary checks, and an opt-in live interaction test:

```powershell
python tools/vanilla-gameplay.py 1.7.2 --interaction-regression `
  --resources PATH_TO_PREPARED_26_3_RESOURCES `
  --client dist/play-0.8.1/horizon-client.exe `
  --ctl dist/play-0.8.1/horizon-ctl.exe
```

The live test right-clicks stone, a sword, food and an empty hand repeatedly.
It then holds the placement packet in the loopback proxy before forwarding
it to vanilla and samples the client's block while no server reply is
possible. After forwarding the packet, it verifies server acceptance and
checks that an authoritative removal replaces the local block. The normal
gameplay checks continue through crafting, containers, combat, respawn,
Nether travel and reconnect.

Artifact paths under `target/` refer to local validation output. They are not
included in this public repository because test runs may contain player or
server metadata. The committed reports in [validation/0.8.2](validation/0.8.2/)
provide the public measurements and build hashes for the current release.
