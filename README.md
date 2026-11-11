# ViaHorizon

Choose the Minecraft server version when playing with Horizon 26.3.
Supports Release servers from 1.7.2 through 26.3.

This fork adds a [Minecraft 1.21.11 entity metadata fix](docs/releases/0.8.8-dev.1.md)
to ViaHorizon 0.8.8-dev.

## Install

Download `viahorizon-0.8.8-dev.1.hnmod` from the
[fork release](https://github.com/PigeonMuyz/viahorizon/releases/tag/v0.8.8-dev.1).
Requires Horizon Launcher with native-build support, prepared 26.3 resources,
Python 3.11+, Rust 1.95+ and a C toolchain.

```powershell
python -m zipfile -e .\viahorizon-0.8.8-dev.1.hnmod .\viahorizon-0.8.8-dev.1
horizon-launcher native-build .\viahorizon-0.8.8-dev.1 --trust-native-code
```

## Build

Use Python 3.11+, Git, Rust 1.99.1 and a C toolchain. Clone Horizon and
horizon-mixins at the commits in [compatibility.json](compatibility.json),
then build horizon-mixins with `cargo build --locked` in its checkout.
On Linux, install `libasound2-dev`, `libudev-dev` and `pkg-config` first.

```powershell
cargo test --locked
python tools/native.py check --horizon PATH_TO_HORIZON --mixins PATH_TO_MIXINS_EXE
python tools/native.py build --horizon PATH_TO_HORIZON --mixins PATH_TO_MIXINS_EXE
python tools/native.py package
```

The package and checksum are written to `dist/`. See the
[publishing guide](docs/publishing/README.md) for release checks and uploads.

## License

[GPL-3.0-or-later](LICENSE). This is an unofficial port of
[ViaFabricPlus](https://github.com/ViaVersion/ViaFabricPlus); upstream authors
and derived protocol data are credited in [third-party notices](THIRD_PARTY_NOTICES.md).
The package includes both notices. No Minecraft assets or Java runtime are bundled.
