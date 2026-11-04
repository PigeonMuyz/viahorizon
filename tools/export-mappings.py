#!/usr/bin/env python3
"""Export ViaVersion's mapping data to Rust without loading a JVM.

Input is the GPL ViaVersion source checkout pinned in THIRD_PARTY_NOTICES.
The output contains data and packet names, never Java bytecode or executables.
"""
import argparse
import hashlib
import re
import struct
from pathlib import Path


class Reader:
    def __init__(self, data):
        self.data, self.pos = data, 0

    def take(self, count):
        if count < 0 or self.pos + count > len(self.data):
            raise ValueError("Truncated mapping data")
        out = self.data[self.pos:self.pos + count]
        self.pos += count
        return out

    def number(self, kind):
        return struct.unpack(">" + kind, self.take(struct.calcsize(kind)))[0]

    def string(self):
        return self.take(self.number("H")).decode("utf-8")

    def tag(self, kind, depth=0):
        if depth > 64:
            raise ValueError("NBT nesting exceeds 64")
        if kind in range(1, 7):
            return self.number({1:"b", 2:"h", 3:"i", 4:"q", 5:"f", 6:"d"}[kind])
        if kind == 7:
            return self.take(self.number("i"))
        if kind == 8:
            return self.string()
        if kind == 9:
            element, count = self.number("B"), self.number("i")
            if not 0 <= count <= 1_000_000:
                raise ValueError("Invalid list size")
            return [self.tag(element, depth + 1) for _ in range(count)]
        if kind == 10:
            out = {}
            while (child := self.number("B")):
                name = self.string()
                if name in out:
                    raise ValueError("Duplicate NBT key")
                out[name] = self.tag(child, depth + 1)
            return out
        if kind in (11, 12):
            count = self.number("i")
            if not 0 <= count <= 1_000_000:
                raise ValueError("Invalid array size")
            return [self.number("i" if kind == 11 else "q") for _ in range(count)]
        raise ValueError(f"Unknown NBT tag: {kind}")

    def varint(self):
        value = 0
        for index in range(5):
            byte = self.number("B")
            value |= (byte & 127) << (7 * index)
            if byte < 128:
                return value
        raise ValueError("Invalid VarInt")

    def zigzag(self):
        value = self.varint()
        return (value >> 1) ^ -(value & 1)


def load(path):
    data = path.read_bytes()
    r = Reader(data)
    if r.number("B") != 10:
        raise ValueError("Expected compound root")
    r.string()
    root = r.tag(10)
    if r.pos != len(data) or root["version"] != 2:
        raise ValueError("Unsupported mapping format")
    return root


def decode(tag):
    size, mapped = tag["size"], tag["mappedSize"]
    if not 0 <= size <= 1_000_000 or not 0 <= mapped <= 1_000_000:
        raise ValueError("Invalid mapping dimensions")
    r = Reader(tag.get("val", b""))
    strategy = tag["id"]
    if strategy == 0:
        values, previous = [], 0
        for _ in range(size):
            previous += r.zigzag()
            values.append(previous)
    elif strategy in (1, 2):
        pairs, at, value = [], -1, 0
        while r.pos < len(r.data):
            at += 1 + r.varint()
            value += r.zigzag()
            if not 0 <= at < size:
                raise ValueError("Invalid change index")
            pairs.append((at, value))
        values = list(range(size)) if strategy == 1 or "nofill" not in tag else [-1] * size
        for index, (at, value) in enumerate(pairs):
            end = pairs[index + 1][0] if index + 1 < len(pairs) else size
            if strategy == 1:
                values[at:end] = range(value, value + end - at)
            else:
                values[at] = value
    elif strategy == 3:
        values = list(range(size))
    else:
        raise ValueError(f"Unknown mapping strategy: {strategy}")
    if r.pos != len(r.data) or any(v < -1 or v >= mapped for v in values):
        raise ValueError("Invalid decoded mappings")
    return values, mapped


def runs(values):
    out = []
    for index, value in enumerate(values):
        if not index or value < 0 or values[index - 1] < 0 or value != values[index - 1] + 1:
            out.append((index, value))
    return out


def packet_names(path):
    # Java's ordinal is the ID; comments are deliberately ignored (some are stale).
    text = re.sub(r"/\*.*?\*/|//[^\n]*", "", path.read_text(), flags=re.S)
    body = text.split("{", 1)[1].split(";", 1)[0]
    names = re.findall(r"\b([A-Z][A-Z0-9_]+)\b", body)
    if len(names) != len(set(names)) or not names:
        raise ValueError(f"Invalid packet enum: {path}")
    return names


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("checkout", type=Path)
    args = parser.parse_args()
    root = args.checkout / "common/src/main"
    data_path = root / "resources/assets/viaversion/data/mappings-26.2to26.3.nbt"
    source = load(data_path)
    output = ["// SPDX-License-Identifier: GPL-3.0-or-later",
              "// Generated by tools/export-mappings.py; do not edit.",
              f"// Input SHA256: {hashlib.sha256(data_path.read_bytes()).hexdigest()}",
              "use crate::vfp_wire::IdMap;"]
    for name, tag in sorted(source.items()):
        if not isinstance(tag, dict) or "mappedSize" not in tag:
            continue
        values, mapped = decode(tag)
        entries = ",".join(f"({at},{to})" for at, to in runs(values))
        symbol = name.replace("-", "_").upper()
        output.append(f"pub const {symbol}: IdMap = IdMap {{ size:{len(values)}, mapped_size:{mapped}, runs:&[{entries}] }};")
    # Resolve the few semantic item lists against the same versioned identifier
    # tables as the numeric maps; never rely on hardcoded current item ids.
    pool_reader = Reader((root / "resources/assets/viaversion/data/identifier-table.nbt").read_bytes())
    if pool_reader.number("B") != 10:
        raise ValueError("Invalid identifier table")
    pool_reader.string()
    pool = pool_reader.tag(10)
    def item_ids(version):
        ids = load(root / f"resources/assets/viaversion/data/identifiers-{version}.nbt")["items"]
        indexes, _ = decode(dict(ids, mappedSize=len(pool["items"])))
        return {pool["items"][value]: index for index, value in enumerate(indexes)}
    old_items, new_items = item_ids("26.2"), item_ids("26.3")
    def registry_ids(version, name):
        ids = load(root / f"resources/assets/viaversion/data/identifiers-{version}.nbt")[name]
        indexes, _ = decode(dict(ids, mappedSize=len(pool[name])))
        return [pool[name][value] for value in indexes]
    # Missing maps are identities only after checking both identifier tables.
    for name in ("attributes", "blockentities"):
        before, after = registry_ids("26.2", name), registry_ids("26.3", name)
        if name not in source:
            if before != after:
                raise ValueError(f"Missing non-identity {name} map")
            size = len(before)
            output.append(f"pub const {name.upper()}: IdMap = IdMap {{size:{size},mapped_size:{size},runs:&[(0,0)]}};")
            source[name] = dict(id=3,size=size,mappedSize=size)
    output.append(f'pub const BRICK_262: i32 = {old_items["brick"]};')
    mapping_java = (root / "java/com/viaversion/viaversion/protocols/v26_2to26_3/data/MappingData26_3.java").read_text()
    for symbol in ("BREWING_INPUTS", "BREWING_REAGENTS"):
        body = re.search(rf'{symbol}\s*=\s*\{{(.*?)\}}', mapping_java, re.S).group(1)
        names = re.findall(r'"([a-z_]+)"', body)
        if not names:
            raise ValueError(f"Missing {symbol}")
        output.append(f'pub const {symbol}: &[i32] = &[' + ','.join(str(new_items[name]) for name in names) + '];')
    base = root / "java/com/viaversion/viaversion/protocols"
    enums = {
        "OLD_CLIENTBOUND": "v1_21_11to26_1/packet/ClientboundPackets26_1.java",
        "NEW_CLIENTBOUND": "v26_2to26_3/packet/ClientboundPackets26_3.java",
        "OLD_SERVERBOUND": "v1_21_11to26_1/packet/ServerboundPackets26_1.java",
        "NEW_SERVERBOUND": "v26_2to26_3/packet/ServerboundPackets26_3.java",
        "OLD_CONFIGURATION": "v1_21_7to1_21_9/packet/ClientboundConfigurationPackets1_21_9.java",
        "NEW_CONFIGURATION": "v26_2to26_3/packet/ClientboundConfigurationPackets26_3.java",
    }
    for symbol, path in enums.items():
        output.append(f"pub const {symbol}: &[&str] = &[" + ",".join(f'"{name}"' for name in packet_names(base / path)) + "];")
    destination = Path(__file__).resolve().parents[1] / "mixins/viahorizon/tables.rs"
    output.append('pub const BLOCK_ENTITY_IDENTIFIERS: &[&str] = &['+','.join('"'+name+'"' for name in registry_ids('26.2','blockentities'))+'];')
    destination.write_text("\n".join(output) + "\n", encoding="utf-8")
    print(f"Exported {destination}")
    registry = root / "resources/assets/viaversion/data/block-transformer-registry-26.3.nbt"
    data = registry.read_bytes()
    reader = Reader(data)
    if reader.number("B") != 10:
        raise ValueError("Invalid block-transformer registry root")
    reader.string()
    content = reader.tag(10)
    if list(content) != ["minecraft:axe", "minecraft:hoe", "minecraft:shovel"] or reader.pos != len(data):
        raise ValueError("Unexpected block-transformer registry order")
    resource = destination.with_name("registry_data.rs")
    resource.write_text("// SPDX-License-Identifier: GPL-3.0-or-later\n"
                        "// Generated by tools/export-mappings.py from ViaVersion data.\n"
                        f"// SHA256: {hashlib.sha256(data).hexdigest()}\n"
                        "pub const BLOCK_TRANSFORMERS: &[u8] = &[" + ",".join(map(str, data)) + "];\n", encoding="utf-8")
    print(f"Exported {resource} ({len(data)} source bytes)")
    previous_path = root / "resources/assets/viaversion/data/mappings-26.1to26.2.nbt"
    previous = ["// SPDX-License-Identifier: GPL-3.0-or-later",
                "// Generated by tools/export-mappings.py; do not edit.",
                f"// Input SHA256: {hashlib.sha256(previous_path.read_bytes()).hexdigest()}",
                "use crate::vfp_wire::IdMap;"]
    for name, tag in sorted(load(previous_path).items()):
        if not isinstance(tag, dict) or "mappedSize" not in tag:
            continue
        values, mapped = decode(tag)
        entries = ",".join(f"({at},{to})" for at,to in runs(values))
        previous.append(f"pub const {name.replace('-','_').upper()}: IdMap = IdMap {{size:{len(values)},mapped_size:{mapped},runs:&[{entries}]}};")
    previous_out = destination.with_name("tables_261.rs")
    previous_out.write_text("\n".join(previous)+"\n",encoding="utf-8")
    print(f"Exported {previous_out}")
    # Direct registry maps for 26.1 -> native. Structural packet changes are
    # handled by the typed translator; composing ids never bypasses those.
    previous_maps = load(previous_path)
    native = ["// SPDX-License-Identifier: GPL-3.0-or-later",
              "// Generated 26.1 -> 26.2 -> 26.3 registry maps.",
              f"// First input SHA256: {hashlib.sha256(previous_path.read_bytes()).hexdigest()}",
              f"// Second input SHA256: {hashlib.sha256(data_path.read_bytes()).hexdigest()}",
              "use crate::vfp_wire::IdMap;"]
    mapped = {k:v for k,v in source.items() if isinstance(v,dict) and "mappedSize" in v}
    composed_maps = {}
    for name in sorted(set(mapped) | {k for k,v in previous_maps.items() if isinstance(v,dict) and "mappedSize" in v}):
        earlier = previous_maps.get(name)
        later = mapped.get(name)
        if later is None:
            size = earlier["mappedSize"]
            later = dict(id=3,size=size,mappedSize=size)
        if earlier is None:
            size = later["size"]
            earlier = dict(id=3,size=size,mappedSize=size)
        first, middle_size = decode(earlier)
        second, native_size = decode(later)
        if middle_size != len(second):
            raise ValueError(f"Incompatible mapping chain for {name}")
        values = [-1 if value < 0 else second[value] for value in first]
        composed_maps[name] = (values, native_size)
        entries = ",".join(f"({at},{to})" for at,to in runs(values))
        native.append(f"pub const {name.replace('-','_').upper()}: IdMap = IdMap {{size:{len(values)},mapped_size:{native_size},runs:&[{entries}]}};")
    native.append(f'pub const BRICK_OLD: i32 = {item_ids("26.1")["brick"]};')
    native.append('pub const BLOCK_ENTITY_IDENTIFIERS: &[&str] = &['+','.join('"'+name+'"' for name in registry_ids('26.1','blockentities'))+'];')
    entity_names = registry_ids("26.3", "entities")
    native.append('pub const ENTITY_NAMES: &[&str] = &[' + ','.join('"'+n+'"' for n in entity_names) + '];')
    native.append('pub const PARTICLE_NAMES: &[&str] = &[' + ','.join('"'+n+'"' for n in registry_ids('26.3', 'particles')) + '];')
    # root is common/src/main; the API crate is a sibling of common.
    enum_source = args.checkout / "api/src/main/java/com/viaversion/viaversion/api/minecraft/entities/EntityTypes26_3.java"
    text = re.sub(r"/\*.*?\*/|//[^\n]*", "", enum_source.read_text(), flags=re.S)
    parents = dict(re.findall(r"\b([A-Z][A-Z0-9_]+)\(([A-Z][A-Z0-9_]+)", text))
    for symbol, parent in [("PROJECTILES", "PROJECTILE"),("CUBE_MOBS", "ABSTRACT_CUBE_MOB"),("AGEABLES", "ABSTRACT_AGEABLE"),("AVATARS", "AVATAR")]:
        def is_child(name):
            seen=set()
            while name not in seen:
                if name == parent: return True
                seen.add(name)
                if name not in parents: return False
                name=parents[name]
            raise ValueError("Cyclic entity ancestry")
        ids=[i for i,name in enumerate(entity_names) if is_child(name.upper())]
        if not ids: raise ValueError(f"Missing {symbol}")
        native.append(f"pub const {symbol}: &[i32] = &["+','.join(map(str,ids))+'];')
    native_out = destination.with_name("tables_261_native.rs")
    native_out.write_text("\n".join(native)+"\n",encoding="utf-8")
    print(f"Exported {native_out}")
    oldest_path = root / "resources/assets/viaversion/data/mappings-1.21.11to26.1.nbt"
    oldest_maps = load(oldest_path)
    old_lines = ["// SPDX-License-Identifier: GPL-3.0-or-later",
                 "// Generated 1.21.11 -> 26.1 steps and composed native maps.",
                 f"// Input SHA256: {hashlib.sha256(oldest_path.read_bytes()).hexdigest()}",
                 "use crate::vfp_wire::IdMap;"]
    extra_domains = {name for name, value in oldest_maps.items() if isinstance(value, dict) and "mappedSize" in value}
    source_identifiers = load(root / "resources/assets/viaversion/data/identifiers-1.21.11.nbt")
    composed_12111 = {}
    for name in sorted(set(composed_maps) | extra_domains):
        if name in composed_maps:
            second, native_size = composed_maps[name]
        else:
            native_size = oldest_maps[name]["mappedSize"]
            second = list(range(native_size))
        earlier = oldest_maps.get(name)
        if earlier is None:
            source_size = len(registry_ids("1.21.11", name)) if name in source_identifiers else len(second)
            first = list(range(source_size))
            middle_size = len(second)
        else:
            first, middle_size = decode(earlier)
        if middle_size != len(second):
            raise ValueError(f"Incompatible 1.21.11 mapping chain for {name}")
        values = [-1 if value < 0 else second[value] for value in first]
        composed_12111[name] = (values, native_size)
        symbol = name.replace('-','_').upper()
        for prefix, entries, size in [("STEP_", first, middle_size),("", values, native_size)]:
            encoded = ','.join(f'({at},{to})' for at,to in runs(entries))
            old_lines.append(f'pub const {prefix}{symbol}: IdMap = IdMap {{size:{len(entries)},mapped_size:{size},runs:&[{encoded}]}};')
    components = registry_ids("1.21.11", "data_component_type")
    for name in ["map_color", "swing_animation"]:
        old_lines.append(f'pub const {name.upper()}_OLD: i32 = {components.index(name)};')
    old_lines.append(f'pub const BED_OLD: i32 = {registry_ids("1.21.11", "blockentities").index("bed")};')
    old_lines.append(f'pub const BRICK_OLD: i32 = {item_ids("1.21.11")["brick"]};')
    old_lines.append('pub const BLOCK_ENTITY_IDENTIFIERS: &[&str] = &['+','.join('"'+name+'"' for name in registry_ids('1.21.11','blockentities'))+'];')
    for symbol, relative in {
        "CLIENTBOUND": "v1_21_9to1_21_11/packet/ClientboundPackets1_21_11.java",
        "SERVERBOUND": "v1_21_5to1_21_6/packet/ServerboundPackets1_21_6.java",
        "CONFIGURATION_CLIENTBOUND": "v1_21_7to1_21_9/packet/ClientboundConfigurationPackets1_21_9.java",
        "CONFIGURATION_SERVERBOUND": "v1_21_7to1_21_9/packet/ServerboundConfigurationPackets1_21_9.java",
    }.items():
        old_lines.append(f'pub const {symbol}: &[&str] = &[' + ','.join('"'+name+'"' for name in packet_names(base / relative)) + '];')
    for symbol, resource in [("FLUID_RANGES_261", "fluids-26.1.nbt"),("SOUND_VARIANTS_261", "sound-variant-registries-26.1.nbt")]:
        blob = (root / "resources/assets/viaversion/data" / resource).read_bytes()
        if symbol == "FLUID_RANGES_261":
            reader = Reader(blob)
            if reader.number('B') != 10: raise ValueError('Invalid fluid registry')
            reader.string()
            blob = reader.tag(10)['fluids']
        old_lines.append(f'// {resource} SHA256: {hashlib.sha256((root / "resources/assets/viaversion/data" / resource).read_bytes()).hexdigest()}')
        old_lines.append(f'pub const {symbol}: &[u8] = &[' + ','.join(map(str, blob)) + '];')
        if symbol == "SOUND_VARIANTS_261":
            reader = Reader(blob)
            if reader.number('B') != 10: raise ValueError('Invalid sound variant registry')
            reader.string()
            registries = reader.tag(10)
            entries = []
            for name, registry in registries.items():
                entries.append('("'+name+'",&['+','.join('"'+key+'"' for key in registry)+'])')
            old_lines.append('pub const SOUND_VARIANT_ENTRY_ORDER: &[(&str,&[&str])] = &['+','.join(entries)+'];')
    old_out = destination.with_name("tables_12111.rs")
    old_out.write_text('\n'.join(old_lines)+'\n',encoding='utf-8')
    print(f"Exported {old_out}")
    path_1219 = root / "resources/assets/viaversion/data/mappings-1.21.9to1.21.11.nbt"
    maps_1219 = load(path_1219)
    ids_1219 = load(root / "resources/assets/viaversion/data/identifiers-1.21.9.nbt")
    lines_1219 = ['// SPDX-License-Identifier: GPL-3.0-or-later',
                  '// Generated 1.21.9 -> 1.21.11 steps and composed native maps.',
                  f'// Input SHA256: {hashlib.sha256(path_1219.read_bytes()).hexdigest()}',
                  'use crate::vfp_wire::IdMap;']
    composed_1219 = {}
    for name, (second, native_size) in sorted(composed_12111.items()):
        earlier = maps_1219.get(name)
        if earlier is None:
            size = len(registry_ids('1.21.9', name)) if name in ids_1219 else len(second)
            first, middle_size = list(range(size)), len(second)
        else:
            first, middle_size = decode(earlier)
        if middle_size != len(second): raise ValueError(f'Incompatible 1.21.9 map {name}')
        values = [-1 if value < 0 else second[value] for value in first]
        composed_1219[name] = (values, native_size)
        symbol = name.replace('-','_').upper()
        for prefix, values, size in [('STEP_', first, middle_size), ('', values, native_size)]:
            encoded=','.join(f'({at},{to})' for at,to in runs(values))
            lines_1219.append(f'pub const {prefix}{symbol}: IdMap = IdMap {{size:{len(values)},mapped_size:{size},runs:&[{encoded}]}};')
    names = registry_ids('1.21.9', 'data_component_type')
    lines_1219.append(f'pub const MAP_COLOR_OLD: i32 = {names.index("map_color")};')
    lines_1219.append(f'pub const BED_OLD: i32 = {registry_ids("1.21.9", "blockentities").index("bed")};')
    lines_1219.append(f'pub const BRICK_OLD: i32 = {item_ids("1.21.9")["brick"]};')
    lines_1219.append('pub const BLOCK_ENTITY_IDENTIFIERS: &[&str] = &['+','.join('"'+name+'"' for name in registry_ids('1.21.9','blockentities'))+'];')
    packets = packet_names(base / 'v1_21_7to1_21_9/packet/ClientboundPackets1_21_9.java')
    lines_1219.append('pub const CLIENTBOUND: &[&str] = &['+','.join('"'+p+'"' for p in packets)+'];')
    timeline = (root / 'resources/assets/viaversion/data/timeline-registry-1.21.11.nbt').read_bytes()
    lines_1219.append(f'// Timeline SHA256: {hashlib.sha256(timeline).hexdigest()}')
    lines_1219.append('pub const TIMELINE: &[u8] = &['+','.join(map(str,timeline))+'];')
    reader = Reader(timeline)
    if reader.number('B') != 10: raise ValueError('Invalid timeline registry')
    reader.string()
    timelines = reader.tag(10)
    lines_1219.append('pub const TIMELINE_ENTRY_ORDER: &[&str] = &['+','.join('"'+key+'"' for key in timelines)+'];')
    output_1219 = destination.with_name('tables_1219.rs')
    output_1219.write_text('\n'.join(lines_1219)+'\n',encoding='utf-8')
    print(f'Exported {output_1219}')
    def export_older(version, next_version, following, stem, packet_files=None):
        path = root / f'resources/assets/viaversion/data/mappings-{version}to{next_version}.nbt'
        maps = load(path)
        identifiers = load(root / f'resources/assets/viaversion/data/identifiers-{version}.nbt')
        lines = ['// SPDX-License-Identifier: GPL-3.0-or-later',
                 f'// Generated {version} -> {next_version} steps and composed native maps.',
                 f'// Input SHA256: {hashlib.sha256(path.read_bytes()).hexdigest()}',
                 'use crate::vfp_wire::IdMap;']
        composed = {}
        for name, (second, native_size) in sorted(following.items()):
            earlier = maps.get(name)
            if earlier is None:
                size = len(registry_ids(version, name)) if name in identifiers else len(second)
                first, middle_size = list(range(size)), len(second)
            else:
                first, middle_size = decode(earlier)
            if middle_size != len(second): raise ValueError(f'Incompatible {version} map {name}')
            values = [-1 if value < 0 else second[value] for value in first]
            composed[name] = (values, native_size)
            symbol = name.replace('-','_').upper()
            for prefix, entries, size in [('STEP_', first, middle_size), ('', values, native_size)]:
                encoded = ','.join(f'({at},{to})' for at,to in runs(entries))
                lines.append(f'pub const {prefix}{symbol}: IdMap = IdMap {{size:{len(entries)},mapped_size:{size},runs:&[{encoded}]}};')
        components = registry_ids(version if 'data_component_type' in identifiers else '1.20.3', 'data_component_type')
        # Menu changes do not occur in the later native maps, but an older
        # transition can introduce a menu and must still export its step.
        if 'menus' in maps:
            entries, size = decode(maps['menus'])
            encoded = ','.join(f'({at},{to})' for at,to in runs(entries))
            lines.append(f'pub const STEP_MENUS: IdMap = IdMap {{size:{len(entries)},mapped_size:{size},runs:&[{encoded}]}};')
        lines.append('pub const COMPONENT_IDENTIFIERS: &[&str] = &['+','.join('"'+n+'"' for n in components)+'];')
        lines.append(f'pub const MAP_COLOR_OLD: i32 = {components.index("map_color")};')
        lines.append(f'pub const BED_OLD: i32 = {registry_ids(version, "blockentities").index("bed")};')
        lines.append(f'pub const BRICK_OLD: i32 = {item_ids(version)["brick"]};')
        lines.append(f'pub const SADDLE_OLD: i32 = {item_ids(version)["saddle"]};')
        for symbol, name in [('SHIELD_BLOCK_OLD', 'item.shield.block'), ('SHIELD_BREAK_OLD', 'item.shield.break')]:
            lines.append(f'pub const {symbol}: i32 = {registry_ids(version, "sounds").index(name)};')
        for symbol, registry in [('ENTITY_IDENTIFIERS','entities'),('BLOCK_ENTITY_IDENTIFIERS','blockentities'),('ITEM_IDENTIFIERS','items'),('BLOCK_IDENTIFIERS','blocks'),('RECIPE_IDENTIFIERS','recipe_serializers'),('ATTRIBUTE_IDENTIFIERS','attributes'),('SOUND_IDENTIFIERS','sounds')]:
            registry_version = version if registry in identifiers else '1.20.3'
            lines.append(f'pub const {symbol}: &[&str] = &['+','.join('"'+n+'"' for n in registry_ids(registry_version, registry))+'];')
        entity_version = '1.19.4' if version in ('1.20', '1.20.2') else version
        entity_source=args.checkout / ('api/src/main/java/com/viaversion/viaversion/api/minecraft/entities/EntityTypes'+entity_version.replace('.','_')+'.java')
        parents=dict(re.findall(r'^\s*([A-Z][A-Z0-9_]*)\((null|[A-Z][A-Z0-9_]*)(?:,[^\n]*)?\)[,;]',entity_source.read_text(),re.M)) if entity_source.exists() else {}
        flags=[]
        for entity in registry_ids(version,'entities'):
            node=entity.upper(); ancestry=set()
            while node in parents and node not in ancestry:
                ancestry.add(node);node=parents[node]
            flags.append(sum(bit for bit,parent in [(1,'LIVING_ENTITY'),(2,'ABSTRACT_HORSE'),(4,'LLAMA'),(8,'ITEM_PROJECTILE'),(16,'ARROW'),(32,'AREA_EFFECT_CLOUD'),(64,'DISPLAY'),(128,'ITEM_DISPLAY')] if parent in ancestry))
        lines.append('pub const ENTITY_FAMILIES: &[u8] = &['+','.join(map(str,flags))+'];')
        packet_files = packet_files or {
            'CLIENTBOUND': 'v1_21_5to1_21_6/packet/ClientboundPackets1_21_6.java',
            'SERVERBOUND': 'v1_21_5to1_21_6/packet/ServerboundPackets1_21_6.java',
            'CONFIGURATION_CLIENTBOUND': 'v1_21_5to1_21_6/packet/ClientboundConfigurationPackets1_21_6.java',
            'CONFIGURATION_SERVERBOUND': 'v1_21_5to1_21_6/packet/ServerboundConfigurationPackets1_21_6.java',
        }
        for symbol, relative in packet_files.items():
            names = packet_names(base / relative)
            lines.append(f'pub const {symbol}: &[&str] = &['+','.join('"'+p+'"' for p in names)+'];')
        output = destination.with_name(f'tables_{stem}.rs')
        output.write_text('\n'.join(lines)+'\n',encoding='utf-8')
        print(f'Exported {output}')
        return composed
    composed_1217 = export_older('1.21.7', '1.21.9', composed_1219, '1217')
    composed_1216 = export_older('1.21.6', '1.21.7', composed_1217, '1216')
    composed_1215 = export_older('1.21.5', '1.21.6', composed_1216, '1215', {
        'CLIENTBOUND': 'v1_21_4to1_21_5/packet/ClientboundPackets1_21_5.java',
        'SERVERBOUND': 'v1_21_4to1_21_5/packet/ServerboundPackets1_21_5.java',
        'CONFIGURATION_CLIENTBOUND': 'v1_20_5to1_21/packet/ClientboundConfigurationPackets1_21.java',
        'CONFIGURATION_SERVERBOUND': 'v1_20_3to1_20_5/packet/ServerboundConfigurationPackets1_20_5.java',
    })
    composed_1214 = export_older('1.21.4', '1.21.5', composed_1215, '1214', {
        'CLIENTBOUND': 'v1_21to1_21_2/packet/ClientboundPackets1_21_2.java',
        'SERVERBOUND': 'v1_21_2to1_21_4/packet/ServerboundPackets1_21_4.java',
        'CONFIGURATION_CLIENTBOUND': 'v1_20_5to1_21/packet/ClientboundConfigurationPackets1_21.java',
        'CONFIGURATION_SERVERBOUND': 'v1_20_3to1_20_5/packet/ServerboundConfigurationPackets1_20_5.java',
    })
    composed_1212 = export_older('1.21.2', '1.21.4', composed_1214, '1212', {
        'CLIENTBOUND': 'v1_21to1_21_2/packet/ClientboundPackets1_21_2.java',
        'SERVERBOUND': 'v1_21to1_21_2/packet/ServerboundPackets1_21_2.java',
        'CONFIGURATION_CLIENTBOUND': 'v1_20_5to1_21/packet/ClientboundConfigurationPackets1_21.java',
        'CONFIGURATION_SERVERBOUND': 'v1_20_3to1_20_5/packet/ServerboundConfigurationPackets1_20_5.java',
    })
    composed_121 = export_older('1.21', '1.21.2', composed_1212, '121', {
        'CLIENTBOUND': 'v1_20_5to1_21/packet/ClientboundPackets1_21.java',
        'SERVERBOUND': 'v1_20_3to1_20_5/packet/ServerboundPackets1_20_5.java',
        'CONFIGURATION_CLIENTBOUND': 'v1_20_5to1_21/packet/ClientboundConfigurationPackets1_21.java',
        'CONFIGURATION_SERVERBOUND': 'v1_20_3to1_20_5/packet/ServerboundConfigurationPackets1_20_5.java',
    })
    composed_1205 = export_older('1.20.5', '1.21', composed_121, '1205', {
        'CLIENTBOUND': 'v1_20_3to1_20_5/packet/ClientboundPackets1_20_5.java',
        'SERVERBOUND': 'v1_20_3to1_20_5/packet/ServerboundPackets1_20_5.java',
        'CONFIGURATION_CLIENTBOUND': 'v1_20_3to1_20_5/packet/ClientboundConfigurationPackets1_20_5.java',
        'CONFIGURATION_SERVERBOUND': 'v1_20_3to1_20_5/packet/ServerboundConfigurationPackets1_20_5.java',
    })
    composed_1203 = export_older('1.20.3','1.20.5',composed_1205,'1203',{
        'CLIENTBOUND':'v1_20_2to1_20_3/packet/ClientboundPackets1_20_3.java',
        'SERVERBOUND':'v1_20_2to1_20_3/packet/ServerboundPackets1_20_3.java',
        'CONFIGURATION_CLIENTBOUND':'v1_20_2to1_20_3/packet/ClientboundConfigurationPackets1_20_3.java',
        'CONFIGURATION_SERVERBOUND':'v1_20to1_20_2/packet/ServerboundConfigurationPackets1_20_2.java',
    })
    composed_1202 = export_older('1.20.2','1.20.3',composed_1203,'1202',{
        'CLIENTBOUND':'v1_20to1_20_2/packet/ClientboundPackets1_20_2.java',
        'SERVERBOUND':'v1_20to1_20_2/packet/ServerboundPackets1_20_2.java',
        'CONFIGURATION_CLIENTBOUND':'v1_20to1_20_2/packet/ClientboundConfigurationPackets1_20_2.java',
        'CONFIGURATION_SERVERBOUND':'v1_20to1_20_2/packet/ServerboundConfigurationPackets1_20_2.java',
    })
    composed_120 = export_older('1.20','1.20.2',composed_1202,'120',{
        'CLIENTBOUND':'v1_19_3to1_19_4/packet/ClientboundPackets1_19_4.java',
        'SERVERBOUND':'v1_19_3to1_19_4/packet/ServerboundPackets1_19_4.java',
    })
    composed_1194 = export_older('1.19.4','1.20',composed_120,'1194',{
        'CLIENTBOUND':'v1_19_3to1_19_4/packet/ClientboundPackets1_19_4.java',
        'SERVERBOUND':'v1_19_3to1_19_4/packet/ServerboundPackets1_19_4.java',
    })
    export_older('1.19.3','1.19.4',composed_1194,'1193',{
        'CLIENTBOUND':'v1_19_1to1_19_3/packet/ClientboundPackets1_19_3.java',
        'SERVERBOUND':'v1_19_1to1_19_3/packet/ServerboundPackets1_19_3.java',
    })
    damage_1194 = (root/'resources/assets/viaversion/data/damage-types-1.19.4.nbt').read_bytes()
    destination.with_name('registry_1193.rs').write_text(
        '// SPDX-License-Identifier: GPL-3.0-or-later\n'
        '// Generated 1.19.3 -> 1.19.4 damage registry.\n'
        f'// Input SHA256: {hashlib.sha256(damage_1194).hexdigest()}\n'
        'pub const DAMAGE_TYPES: &[u8] = &['+','.join(map(str,damage_1194))+'];\n',encoding='utf-8')
    legacy = ['// SPDX-License-Identifier: GPL-3.0-or-later', '// Generated registry records and modifier identities for 1.20.5/1.20.6.']
    for symbol, filename in [('ENCHANTMENTS','enchantments-1.21.nbt'),('JUKEBOX_SONGS','jukebox-songs-1.21.nbt')]:
        data = (root / 'resources/assets/viaversion/data' / filename).read_bytes()
        legacy += [f'// {filename} SHA256: {hashlib.sha256(data).hexdigest()}',
                   f'pub const {symbol}: &[u8] = &['+','.join(map(str,data))+'];']
        if symbol == 'JUKEBOX_SONGS':
            reader=Reader(data)
            if reader.number('B') != 10: raise ValueError('Jukebox registry root')
            reader.string()
            legacy.append('pub const JUKEBOX_ORDER: &[&str] = &['+','.join('"'+name+'"' for name in reader.tag(10))+'];')
    painting_source = (base / 'v1_20_5to1_21/data/Paintings1_20_5.java').read_text()
    paintings = re.findall(r'new PaintingVariant\((\d+), (\d+), "([a-z_0-9]+)"\)', painting_source)
    legacy.append('pub const PAINTINGS: &[(i32,i32,&str)] = &['+','.join(f'({w},{h},"{name}")' for w,h,name in paintings)+'];')
    enchants = (base / 'v1_20_3to1_20_5/data/Enchantments1_20_5.java').read_text()
    enchants = enchants.split('KeyMappings.of(',1)[1].split(');',1)[0] if 'KeyMappings.of(' in enchants else enchants.split('new KeyMappings(',1)[1].split(');',1)[0]
    legacy.append('pub const ENCHANTMENT_NAMES: &[&str] = &['+','.join('"'+name+'"' for name in re.findall(r'"([a-z_]+)"',enchants))+'];')
    attributes = (base / 'v1_20_5to1_21/data/AttributeModifierMappings1_21.java').read_text()
    uuids = [(int(msb)&((1<<64)-1))<<64 | (int(lsb)&((1<<64)-1)) for msb,lsb,name in re.findall(r'add\((-?\d+)L, (-?\d+)L, "([^"]+)"\)',attributes)]
    names = [name for msb,lsb,name in re.findall(r'add\((-?\d+)L, (-?\d+)L, "([^"]+)"\)',attributes)]
    legacy.append('pub const MODIFIER_UUIDS: &[(u128,&str)] = &['+','.join(f'({uuid},"{name}")' for uuid,name in zip(uuids,names))+'];')
    name_pairs = re.findall(r'"([^"]+)", "([^"]+)"',attributes.split('Map.of(',1)[1].split('));',1)[0])
    legacy.append('pub const MODIFIER_NAMES: &[(&str,&str)] = &['+','.join(f'("{a}","{b}")' for a,b in name_pairs)+'];')
    destination.with_name('registry_1205.rs').write_text('\n'.join(legacy)+'\n',encoding='utf-8')
    nbt_data=['// SPDX-License-Identifier: GPL-3.0-or-later','// Generated names for legacy item NBT conversion.']
    data_dir=base / 'v1_20_3to1_20_5/data'
    for symbol,filename in [('POTIONS','Potions1_20_5'),('EFFECTS','PotionEffects1_20_5'),('INSTRUMENTS','Instruments1_20_3'),('ARMOR_MATERIALS','ArmorMaterials1_20_5'),
                            ('SLOTS','EquipmentSlots1_20_5'),('ATTRIBUTES','Attributes1_20_5'),('PATTERNS','BannerPatterns1_20_5'),('DECORATIONS','MapDecorations1_20_5')]:
        source=(data_dir/(filename+'.java')).read_text()
        if 'new KeyMappings(' in source: values=source.split('new KeyMappings(',1)[1].split(');',1)[0]
        else: values=source.split('POTION_EFFECTS = {',1)[1].split('};',1)[0]
        names=re.findall(r'"([a-z_0-9.]+)"',values)
        nbt_data.append(f'pub const {symbol}: &[&str] = &['+','.join('"'+name+'"' for name in names)+'];')
    patterns=(data_dir/'BannerPatterns1_20_5.java').read_text()
    pairs=re.findall(r'PATTERN_IDS.put\("([^"]+)", "([^"]+)"\)',patterns)
    nbt_data.append('pub const PATTERN_CODES: &[(&str,&str)] = &['+','.join(f'("{a}","{b}")' for a,b in pairs)+'];')
    damage=(root/'resources/assets/viaversion/data/damage-types-1.20.3.nbt').read_bytes()
    nbt_data += [f'// Damage types SHA256: {hashlib.sha256(damage).hexdigest()}',
                 'pub const DAMAGE_TYPES: &[u8] = &['+','.join(map(str,damage))+'];']
    nbt_data.append('pub const SOUND_NAMES: &[&str] = &['+','.join('"'+name+'"' for name in registry_ids('1.20.3','sounds'))+'];')
    destination.with_name('registry_1203.rs').write_text('\n'.join(nbt_data)+'\n',encoding='utf-8')


if __name__ == "__main__":
    main()
