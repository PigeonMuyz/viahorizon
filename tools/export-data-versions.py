#!/usr/bin/env python3
"""Read Minecraft's version.json as data, without starting a Java runtime.

The release URLs come from Horizon MCP instance_versions. Only ZIP metadata
and version.json are requested when the CDN accepts byte ranges.
"""
import hashlib
import io
import json
from pathlib import Path
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[1]
FAMILIES = [(761, '1.19.3'), (762, '1.19.4'), (763, '1.20'), (764, '1.20.2'), (765, '1.20.3'), (766, '1.20.5'), (767, '1.21'),
            (768, '1.21.2'), (769, '1.21.4'), (770, '1.21.5'),
            (771, '1.21.6'), (772, '1.21.7'), (773, '1.21.9'),
            (774, '1.21.11'), (775, '26.1'), (776, '26.2'), (777, '26.3')]


def request(url, **headers):
    return urllib.request.urlopen(urllib.request.Request(url, headers=headers), timeout=30)


class RemoteZip(io.RawIOBase):
    def __init__(self, url, size):
        self.url, self.size, self.pos = url, size, 0
        self.cache_start, self.cache = 0, b''

    def seekable(self):
        return True

    def readable(self):
        return True

    def tell(self):
        return self.pos

    def seek(self, offset, whence=0):
        self.pos = (0 if whence == 0 else self.pos if whence == 1 else self.size) + offset
        if not 0 <= self.pos <= self.size:
            raise ValueError('ZIP seek outside client archive')
        return self.pos

    def read(self, count=-1):
        count = self.size - self.pos if count < 0 else min(count, self.size - self.pos)
        if count == 0:
            return b''
        if not (self.cache_start <= self.pos and self.pos + count <= self.cache_start + len(self.cache)):
            start, end = self.pos, min(self.size, self.pos + max(count, 65536))
            url = self.url + f'?vfp_range={start}-{end-1}'
            with request(url, Range=f'bytes={start}-{end-1}', **{'Accept-Encoding': 'identity'}) as response:
                if response.status == 206:
                    data = response.read(end-start+1)
                    if len(data) != end-start:
                        raise ValueError('Invalid client archive range length')
                    self.cache_start, self.cache = start, data
                elif response.status == 200:
                    # Some CDNs ignore Range. Bound this fallback to the exact
                    # size advertised by the content-addressed version manifest.
                    data = response.read(self.size+1)
                    if len(data) != self.size:
                        raise ValueError('Invalid client archive size')
                    self.cache_start, self.cache = 0, data
                else:
                    raise ValueError(f'Unexpected client archive status {response.status}')
        at = self.pos-self.cache_start
        self.pos += count
        return self.cache[at:at+count]


def main():
    catalog = {entry['id']: entry for entry in json.loads((ROOT/'tools/data/minecraft-versions.json').read_text())}
    previous = ROOT/'tools/data/minecraft-data-versions.json'
    cached = {entry['release']: entry for entry in json.loads(previous.read_text())} if previous.exists() else {}
    result = []
    for protocol, release in FAMILIES:
        entry = catalog[release]
        known = cached.get(release)
        if known and known['manifest'] == entry['url'] and known['protocol'] == protocol:
            result.append(known)
            continue
        with request(entry['url']) as response:
            raw = response.read(8*1024*1024)
        if hashlib.sha1(raw).hexdigest() != entry['sha1']:
            raise ValueError(f'Minecraft metadata digest differs for {release}')
        manifest = json.loads(raw)
        client = manifest['downloads']['client']
        with zipfile.ZipFile(RemoteZip(client['url'], client['size'])) as archive:
            raw_version = archive.read('version.json')
        version = json.loads(raw_version)
        if version['protocol_version'] != protocol:
            raise ValueError(f'Protocol family differs for {release}')
        result.append(dict(release=release, protocol=protocol, data_version=version['world_version'],
                           manifest=entry['url'], client=client['url'], client_sha1=client['sha1'],
                           version_json_sha256=hashlib.sha256(raw_version).hexdigest()))
        print(f"{release}: protocol {protocol}, data version {version['world_version']}", flush=True)
    (ROOT/'tools/data/minecraft-data-versions.json').write_text(json.dumps(result, indent=2)+'\n', encoding='utf-8')
    lines = ['// SPDX-License-Identifier: GPL-3.0-or-later',
             '// Generated from Mojang version.json data; see tools/data/minecraft-data-versions.json.']
    for entry in result:
        lines += [f"// {entry['release']}, version.json SHA256: {entry['version_json_sha256']}",
                  f"pub const P{entry['protocol']}: i32 = {entry['data_version']};"]
    lines += ['pub const fn for_protocol(protocol: i32) -> Option<i32> {', 'match protocol {']
    lines += [f"{entry['protocol']} => Some(P{entry['protocol']})," for entry in result]
    lines += ['_ => None,', '}', '}']
    (ROOT/'mixins/viahorizon/data_versions.rs').write_text('\n'.join(lines)+'\n', encoding='utf-8')


if __name__ == '__main__':
    main()
