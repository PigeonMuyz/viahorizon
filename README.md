# ViaFabricPlus Native for Horizon 26.3

**ViaFabricPlus Native 0.8.2-dev enables Release servers from 1.7.2 through
26.3.** The client stays on Minecraft **26.3 / protocol 777**. Horizon
translates the older protocols in Rust. The mod and client require no Java
runtime, Fabric, ViaProxy, or translation service. The
[release extension ledger](docs/RELEASE_EXTENSION.md) records validation and
open compatibility cases. Version 0.8.2 removes render-thread waits during legacy chunk translation; see
[streaming validation](docs/PERFORMANCE.md). The earlier right-click and placement
fixes are recorded in [interaction validation](docs/INTERACTION_FIXES.md).

In Options → Mods, make sure **ViaFabricPlus Native is ON**, then open its
settings and choose **Auto** or an explicit supported server version. Keep
**Configuration preview OFF** for gameplay. The
default target remains **Native 26.3**, so selecting a target is necessary when
joining an older server. Reconnect after changing it.

| Server target | Current capability |
| --- | --- |
| 26.3 | Horizon's native protocol, with signed-chat support |
| 1.19.3–26.2 | Enabled login, configuration and gameplay translation; 16 protocol families |
| 1.7.2–1.19.2 | Enabled login and gameplay translation; 35 protocol families |
| 1.0.0–1.6.4 | Catalog entries only; unsupported connections are rejected |
| Beta, Alpha, Classic, Bedrock and snapshots | Outside this project's Release-only scope |

This is an unofficial development port. [Gameplay results](docs/GAMEPLAY_VALIDATION.md)
record the full offline vanilla matrix on 0.7.1-dev. The
[0.7.2 signed-chat report](docs/SIGNED_CHAT.md) records **78 accepted messages on
online-mode 1.19.3, 1.21.1 and 26.3 servers with secure profiles enforced**,
plus 252 passing tests and an offline gameplay regression on 0.7.2-dev.

Online chat requires Horizon Launcher with your Microsoft account selected.
Certificate retrieval and ordinary message signing are automatic.
Signed command arguments such as /msg and /me, custom plugins/datapacks,
and full ViaFabricPlus behavior parity remain incomplete.

## Install

Download `viafabricplus-0.8.2-dev.hnmod` and its `.sha256` file from this
repository's [Releases page](https://github.com/hhhhsffd/viafabricplus-native/releases). The release is a native source bundle compiled
into a separate Horizon instance. Installation requires Horizon Launcher with
native-build support, Python 3.11+, Rust 1.95+ and a C toolchain. The launcher
needs prepared Minecraft 26.3 resources from your own installation/account.

From the directory containing the download:

```powershell
python -m zipfile -e .\viafabricplus-0.8.2-dev.hnmod .\viafabricplus-0.8.2-dev
horizon-launcher native-build .\viafabricplus-0.8.2-dev --trust-native-code
```

For a source checkout, first run `python tools/native.py package`; the generated
package and checksum appear in `dist/`. The `.hnmod` contains Rust sources and
embedded license notices; it is not a Fabric `.jar` or a prebuilt game client.

Then select the launcher's **Native mods** instance and start it. In Options →
Mods, turn **ViaFabricPlus Native 0.8.2-dev (26.3) ON**, open its settings,
select **Auto**, and leave **Configuration preview OFF**. Join the older
server normally. The mod's ON/OFF switch is separate from its target setting;
when the mod is OFF, changing the target does not enable translation.

The bundle pins Horizon commit
`d5f484c89f6e88cee27ad104860587a94b379cd5` and horizon-mixins commit
`3f99a5c60dfa311cc365524c44ae2fd799bfa1de`. The launcher builds those revisions;
this mod hooks networking internals and does not declare hooks-only compatibility.
Revalidate both pins before updating them. GitHub releases are distributed
separately from the Horizon public mod registry.

## Settings and commands

Settings are stored in the instance's game home at:

```text
horizon-mods/.horizon-mod-data/viafabricplus/config.json
```

For example:

```json
{
  "schema": 1,
  "target": "auto",
  "login_preview": false,
  "servers": {
    "example.org:25565": "1.19.3"
  }
}
```

Chat commands are local to the mod:

```text
/vfp status
/vfp target auto
/vfp target native
/vfp target 1.19.3
/vfp server example.org:25565 1.19.3
/vfp forget example.org:25565
/vfp preview off
/vfp probe example.org:25565
/vfp reload
```

`/viafabricplus` is an alias. Configuration preview is an optional diagnostic
mode that intentionally stops before world entry. Turn it off to play.
Auto detection uses Horizon's DNS/SRV resolver and a server status request.
No server is contacted until a probe or connection is requested. Existing
connections keep their selected protocol; integrated single-player connections
are excluded from translation.

## Implementation

The native bundle registers its settings, persists per-server choices, and
installs connection hooks before Horizon's framing, compression and encryption.
Authentication and AES/CFB8 remain Horizon's. Sessions retain the original host
and port, survive reader/writer thread moves, and translate both directions.
Transfer login intents are adapted for servers before 1.20.5.

The Rust chain covers versioned packet IDs, registry ordering, synthesized
configuration for pre-1.20.2 servers, items/NBT/components, recipes and menus,
inventory predictions, chunks/light/block entities, dimensions, entities and
metadata, movement, text/chat, and common gameplay actions. The 0.7 fixes include
dimension-bounded chunk padding, older fluid-section counts, decorated-pot recipe
categories, and the public gameplay session path.

Unknown transformed inventory hashes request server resynchronization. Full
changed/carried item values are captured for older click packets. Some advanced
value/predicate conversions, native pick-item emulation on older servers, and
version-specific behavior remain in the [port ledger](docs/PORTING.md).
Successful ordinary gameplay is not a claim that every protocol feature or
server extension is covered.

## Build and verify

Use Python 3.11+ and Rust 1.95 or newer. The validated Windows build used Rust 1.99.1 GNU. See the
[0.8.2-dev release notes](docs/releases/0.8.2-dev.md) for validation and limitations.

```powershell
python tools/native.py package
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
python tools/native.py build --horizon PATH_TO_PINNED_HORIZON --mixins PATH_TO_HORIZON_MIXINS_EXE
python tools/transport-proof.py
```

The real SDK applies 27 injections and 94 support modules to
`target/horizon-stage`; the original Horizon checkout is not modified.
`build` compiles the client and horizon-ctl and records binary/source hashes in
`target/native-game-build.json`. `check` stages and checks without producing a
game. The standalone diagnostic CLI is `vfp-native`:

```powershell
cargo run --locked -- versions
cargo run --locked -- probe 127.0.0.1:25565
```

Rust fixtures require no Minecraft assets or Java. Real gameplay validation
additionally needs prepared 26.3 resources and Java **only for isolated vanilla
test servers**:

```powershell
python tools/vanilla-matrix.py --resources PATH_TO_PREPARED_26_3 --client target/horizon-stage/target/fast/horizon-client.exe --ctl target/horizon-stage/target/fast/horizon-ctl.exe
```

The driver checks Mojang checksums, uses fresh homes/worlds under `target/`,
records server-authoritative assertions and private transcripts, and stops its
processes. It does not install anything into player instances.

## License and attribution

GPL-3.0-or-later; see [LICENSE](LICENSE) and
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). This is an unofficial port,
not endorsed by ViaVersion, Horizon, Mojang, or Microsoft. No Mojang files or
Java runtime are included in the package.
