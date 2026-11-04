# 0.8.2 chunk-streaming frame pacing

The 0.8.1 client reproduced periodic CPU stalls while exploring a vanilla
1.7.2 world. Rendering and movement queried or acquired the same translation
state mutex that decoded large legacy chunk batches. A slow batch therefore
blocked the game thread even though GPU work was small.

0.8.2 publishes connection capabilities through an atomic snapshot, updated
at actual protocol phase transitions. Render and input hooks read that
snapshot through the session's counters, without locking either its writer
or the translator. For servers before 1.19, an ordered background writer
translates and sends gameplay packets. The queue is bounded to 1,024 packets
and 4 MiB, checks the connection identity before every send, and closes an
overloaded connection instead of silently dropping interactions. Horizon
still owns framing, encryption, compression and the socket.

The matched exploration test used vanilla 1.7.2, seed 1, a six-chunk render
distance, 1280×720, VSync off and a 120 FPS cap on the same RTX 2080 machine.
It measured a stationary interval, then eight 64-block teleports into new
terrain. The patched run used the baseline origin `(164.5, 68, 253.5)`;
recorded first and last sample positions match between runs. Compilation
was complete before measuring the patched run.

| Streaming metric | 0.8.1 | 0.8.2 |
| --- | ---: | ---: |
| Average FPS | 70.0 | 116.1 |
| 1% low FPS | 3.1 | 80.9 |
| 99th-percentile frame time | 201.7 ms | 9.1 ms |
| Longest frame | 644.1 ms | 30.5 ms |

These are measurements of this route and machine, not a guaranteed frame
rate for other servers or graphics settings. Both runs completed without
packet decode errors. Raw measurements:

- [0.8.1 baseline](validation/0.8.2/streaming-0.8.1.json)
- [0.8.2 streaming run](validation/0.8.2/streaming-0.8.2.json)

Reproduce the streaming workload using the matching installed binaries:

```powershell
python tools/vanilla-gameplay.py 1.7.2 --terrain normal --join-only `
  --streaming-profile --profile-origin 164.5 68 253.5 `
  --resources PATH_TO_PREPARED_26_3_RESOURCES `
  --client dist/play-0.8.2/horizon-client.exe `
  --ctl dist/play-0.8.2/horizon-ctl.exe
```

Concurrency regressions exercise capability reads while the translator is
held, phase/protocol capability boundaries, ordered input while the writer
is busy, replacement connections, and queue overflow. The outbox tests compile
the actual injected helper against Horizon's real protocol writer and memory
transport. Live interaction checks retain the previous right-click and
placement tests, then exercise crafting, containers, combat, Nether travel
and reconnect.

All 255 Rust tests passed (46 standalone and 209 host/transport checks).
The installed binaries also passed all 15 live 1.7.2 interaction/gameplay checks
and all 12 live 1.19.2 Auto gameplay checks. An initial 1.19.2 Auto attempt timed
out during version detection while compilation was running; the isolated
rerun passed. The complete provenance and result paths are recorded in
[the public 0.8.2 validation report](validation/0.8.2/summary.json).

Existing users should rebuild their modded instance when upgrading. Updating
only the `.hnmod` file does not replace an older prebuilt client. Restart the
game and launcher after installing a new build.

Artifact paths under `target/` refer to local validation output. They are not
included in this public repository because test runs may contain player or
server metadata. The committed reports in [validation/0.8.2](validation/0.8.2/)
provide the public measurements and build hashes for the current release.
