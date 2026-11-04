# Signed chat correction — 0.7.2-dev, 1 November 2026

The pinned Horizon client authenticated the account for login, but sent ordinary
chat unsigned and never announced a profile chat key. An online server enforcing
secure profiles therefore replied with `chat.disabled.missingProfileKey`.
The preceding offline-mode matrix did not cover this requirement.

The mod now obtains the profile certificate from official Minecraft Services
using Horizon's authenticated session. It uses the already bundled
ureq/rustls/ring provider for RSA-PKCS1-SHA256 and checks the key pair before use.
The private chat key stays in memory and is not logged or saved. Account
credentials retain the launcher's normal one-use session-file handoff.

Each connection announces a fresh chat-session UUID and signs ordinary chat with
its chain index, timestamp, random salt and acknowledged message signatures.
A rolling twenty-message window tracks acknowledgements. Native 26.3 and
translated connections both receive signing support; reconnecting starts a new
chain. Status probes, disabled mods and integrated connections do not acquire
a signing connection. Encryption/compression remains host-owned.

The signed-message field order was checked against official checksum-verified
1.21.1 server bytecode using Horizon's Python class-file reader, without Java.

## Live online-mode tests

The servers were isolated localhost vanilla, with **online-mode=true**,
**enforce-secure-profile=true**, and compression threshold 256. The launcher
used the already selected owning Microsoft account and its OS keychain through
fresh game/launcher homes. The harness copied only public account metadata,
not tokens, and used the normal one-use launcher session mechanism.

| Vanilla release | Authenticated entry | Accepted ordinary chat | After reconnect | Decode errors |
| --- | --- | --- | --- | --- |
| 1.19.3 | Passed | 25 messages | 1 accepted message | 0 |
| 1.21.1 | Passed | 25 messages | 1 accepted message | 0 |
| 26.3 (native) | Passed | 25 messages | 1 accepted message | 0 |

These **78 accepted messages** establish actual certificate retrieval, key
announcement and server-validated signatures with secure profiles enforced,
including the twenty-message window rollover.

Reports: 1.19.3 (`../target/online-chat-1.19.3-a35464a1/result.json`; local artifact),
1.21.1 (`../target/online-chat-1.21.1-b55b93f3/result.json`; local artifact), and
26.3 (`../target/online-chat-26.3-be753fb4/result.json`; local artifact).

The offline 1.21.1 regression passed all twelve gameplay groups:
report (`../target/vplay-1.21.1-d4609468/result.json`; local artifact). All test processes stopped.
Java ran only the authorized isolated vanilla servers. The mod, client and
launch helper remain Rust, and upstream Horizon sources are pristine.

## Automated checks and package

**252 tests passed:** 46 standalone, 202 transformed-protocol/transport, and
4 certificate/provider/chain tests; no selected failures or ignored tests.
They verify real RSA signatures and service-signed fixture certificates,
malformed/expired input rejection without exposing private fields, rolling
acknowledgements, duplicates, ack-only packets, reconnects and native transport
key announcement without retargeting the handshake.

Strict Clippy passes for the standalone project, transformed protocol, and
transformed render library plus signing test, with warnings denied. Changed
Rust files pass rustfmt and Python tools parse. The real SDK applied 15
injections and 73 modules; the native client and ctl built successfully.

Generated keys under `tests/data/` are deliberately public fixtures, not
account certificates. The package excludes them, official jars/resources,
account metadata, logs and runtime keys.

Artifact: `dist/viafabricplus-0.7.2-dev.hnmod` — 559,023 bytes,
90 files. SHA-256: `654acd03d4a6154b8ddd97352a918925fc877b683900f246f17be059e717f204`.
Source digest: `ba0569c06ea3b6d42dff1be6489e8fe588bb4d931ef0671ebc25733e6d5bbc78`.

Machine report (`../target/validation-summary-0.7.2.json`; local artifact) and
package verification (`../target/package-validation-0.7.2.json`; local artifact) record exact
hashes, CRC checks and byte-identical repeated packaging.

The existing **ViaFabricPlus 0.7.0** launcher instance now runs **0.7.2-dev**
from `dist/play/`, retaining its name/id, account handoff and settings.
Previous executables are in `dist/play/previous-0.7.1-dev/`.
The player needs only to relaunch.

## Current limits

This change signs ordinary chat. The pinned host still sends unsigned command
arguments, including message arguments for `/msg` and `/me`; their
signing needs separate work. Certificates refresh on a new connection when
needed; reconnect if a very long session outlasts certificate expiry.
These vanilla tests do not establish every proxy/plugin combination or
complete ViaFabricPlus behavior parity.

Artifact paths under `target/` refer to local validation output. They are not
included in this public repository because test runs may contain player or
server metadata. The committed reports in [validation/0.8.2](validation/0.8.2/)
provide the public measurements and build hashes for the current release.
