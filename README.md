# ViaHorizon

Choose the Minecraft server version when playing with Horizon 26.3.
An unofficial native Rust port of ViaFabricPlus, supporting Release servers
from 1.7.2 through 26.3.

## Install

Download `viahorizon-0.8.4-dev.hnmod` from
[Releases](https://github.com/hhhhsffd/viahorizon/releases).
Requires Horizon Launcher with native-build support, prepared 26.3 resources,
Python 3.11+, Rust 1.95+ and a C toolchain.

```powershell
python -m zipfile -e .\viahorizon-0.8.4-dev.hnmod .\viahorizon-0.8.4-dev
horizon-launcher native-build .\viahorizon-0.8.4-dev --trust-native-code
```

Start the **Native mods** instance through Horizon Launcher. In Options > Mods,
turn **ViaHorizon ON**, choose **Auto** or your server version, and leave
**Configuration preview OFF**. Rebuild the instance when upgrading.
