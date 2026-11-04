#!/usr/bin/env python3
"""Test 1.21.1 gameplay with advisory chunk counters outside 0..4096.

Test-only: mutates the first section counter of vanilla chunk packets at the
offline localhost proxy. Does not touch account traffic or real player data.
Pass vanilla-gameplay.py arguments, starting with 1.21.1, without --auto.
"""
import importlib.util
import json
from pathlib import Path
import re
import sys
import zlib

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('vanilla', ROOT/'tools/vanilla-gameplay.py')
vanilla = importlib.util.module_from_spec(spec)
spec.loader.exec_module(vanilla)
assert len(sys.argv) > 1 and sys.argv[1] == '1.21.1'
assert '--auto' not in sys.argv and '--transfer-from' not in sys.argv
table = (ROOT/'mixins/viahorizon/tables_121.rs').read_text(encoding='utf-8')
names = table.split('pub const CLIENTBOUND: &[&str] = &[', 1)[1].split('];', 1)[0]
CHUNK = re.findall(r'"([A-Z_]+)"', names).index('LEVEL_CHUNK_WITH_LIGHT')
VALUES = [-1, 4097, 8192, -32768, 32767]
records = {}


def varint(value):
    out = bytearray()
    while value > 127:
        out.append((value & 127) | 128)
        value >>= 7
    out.append(value)
    return bytes(out)


def replace_counter(payload, value):
    # Vanilla 1.21.1: x, z, nameless heightmap compound of long arrays,
    # VarInt section-buffer length, then the first section's signed short.
    position = 8
    assert payload[position] == 10, 'Expected heightmap compound'
    position += 1
    while payload[position]:
        assert payload[position] == 12, 'Expected heightmap long array'
        position += 1
        name_size = int.from_bytes(payload[position:position+2], 'big')
        position += 2+name_size
        size = int.from_bytes(payload[position:position+4], 'big', signed=True)
        assert 0 <= size <= 4096
        position += 4+size*8
    position += 1
    size, position = vanilla.TranscriptProxy.varint(payload, position)
    assert size >= 8 and position+size <= len(payload)
    out = bytearray(payload)
    out[position:position+2] = value.to_bytes(2, 'big', signed=True)
    return out


class MutatingDestination:
    def __init__(self, proxy, destination):
        self.proxy, self.destination = proxy, destination

    def sendall(self, frame):
        if self.proxy.phase != 'play':
            self.destination.sendall(frame)
            return
        size, offset = vanilla.TranscriptProxy.varint(frame)
        assert size == len(frame)-offset
        data = frame[offset:]
        compressed_size = None
        if self.proxy.compressed:
            compressed_size, offset = vanilla.TranscriptProxy.varint(data)
            data = zlib.decompress(data[offset:]) if compressed_size else data[offset:]
        identifier, offset = vanilla.TranscriptProxy.varint(data)
        if self.proxy.phase == 'play' and identifier == CHUNK:
            values = records.setdefault(self.proxy.run, [])
            value = VALUES[len(values) % len(VALUES)]
            data = varint(identifier)+replace_counter(data[offset:], value)
            values.append(value)
            (self.proxy.run/'advisory-counts.json').write_text(
                json.dumps({'packets_modified': len(values), 'counts': sorted(set(values))})+'\n',
                encoding='utf-8')
            if compressed_size is not None:
                data = (varint(len(data))+zlib.compress(data) if compressed_size
                        else b'\0'+data)
            frame = varint(len(data))+data
        self.destination.sendall(frame)

    def shutdown(self, how):
        self.destination.shutdown(how)


original_pipe = vanilla.TranscriptProxy.pipe


def pipe(self, source, destination, direction):
    if direction == 'clientbound':
        destination = MutatingDestination(self, destination)
    return original_pipe(self, source, destination, direction)


vanilla.TranscriptProxy.pipe = pipe
vanilla.main()
assert records and all(set(values) == set(VALUES) for values in records.values())
print(json.dumps({'advisory_count_gameplay': True,
                  'chunk_packets_modified': sum(map(len, records.values())),
                  'counter_values': VALUES}), flush=True)
