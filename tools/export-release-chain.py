#!/usr/bin/env python3
"""Generate source data for the 1.7.2--1.19.3 Rust translation chain.

Reads pinned GPL ViaVersion/ViaLegacy sources and NBT, without Java or a build.
Only writes generated Rust protocol data. Generating a descriptor does not
enable a release or establish gameplay compatibility.
"""
import argparse
import hashlib
import importlib.util
import json
import re
import subprocess
from pathlib import Path

VIA = "d8415e6f5991f0b4325207a0477c80e9e3ee1b8f"
LEGACY = "3f7ecf67e74a16aa9b3a0ea9c0b116d072e73618"

# Numeric protocol identifiers are scoped to the Netty Release transport here.
# In particular, 47 means 1.8, never the pre-Netty 1.4.2 transport.
RELEASES = [
    (4, "1.7.2", "r1_7_2_5tor1_7_6_10", None),
    (5, "1.7.6", "r1_7_6_10tor1_8", None),
    (47, "1.8", "v1_8to1_9", None),
    (107, "1.9", "v1_9to1_9_1", None),
    (108, "1.9.1", None, None),
    (109, "1.9.2", "v1_9_1to1_9_3", None),
    (110, "1.9.3", "v1_9_3to1_10", None),
    (210, "1.10", "v1_10to1_11", None),
    (315, "1.11", "v1_11to1_11_1", None),
    (316, "1.11.1", "v1_11_1to1_12", None),
    (335, "1.12", "v1_12to1_12_1", None),
    (338, "1.12.1", "v1_12_1to1_12_2", None),
    (340, "1.12.2", "v1_12_2to1_13", "1.12to1.13"),
    (393, "1.13", "v1_13to1_13_1", "1.13to1.13.2"),
    (401, "1.13.1", "v1_13_1to1_13_2", None),
    (404, "1.13.2", "v1_13_2to1_14", "1.13.2to1.14"),
    (477, "1.14", "v1_14to1_14_1", None),
    (480, "1.14.1", "v1_14_1to1_14_2", None),
    (485, "1.14.2", "v1_14_2to1_14_3", None),
    (490, "1.14.3", "v1_14_3to1_14_4", None),
    (498, "1.14.4", "v1_14_4to1_15", "1.14to1.15"),
    (573, "1.15", "v1_15to1_15_1", None),
    (575, "1.15.1", "v1_15_1to1_15_2", None),
    (578, "1.15.2", "v1_15_2to1_16", "1.15to1.16"),
    (735, "1.16", "v1_16to1_16_1", None),
    (736, "1.16.1", "v1_16_1to1_16_2", "1.16to1.16.2"),
    (751, "1.16.2", "v1_16_2to1_16_3", None),
    (753, "1.16.3", "v1_16_3to1_16_4", None),
    (754, "1.16.4", "v1_16_4to1_17", "1.16.2to1.17"),
    (755, "1.17", "v1_17to1_17_1", None),
    (756, "1.17.1", "v1_17_1to1_18", "1.17to1.18"),
    (757, "1.18", "v1_18to1_18_2", None),
    (758, "1.18.2", "v1_18_2to1_19", "1.18to1.19"),
    (759, "1.19", "v1_19to1_19_1", None),
    (760, "1.19.1", "v1_19_1to1_19_3", "1.19to1.19.3"),
]


def text(path):
    return path.read_text(encoding="utf-8")


def fingerprint(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def mapping_runs(nbt, value):
    # The pre-flattening item map is a sparse (item_id << 4 | data) map.
    # Its `size` counts entries, rather than bounding its numeric keys. The
    # modern exporter's dense-array decoder deliberately rejects that layout.
    if value["id"] != 2 or "nofill" not in value:
        values, mapped = nbt.decode(value)
        return map_runs(values), len(values), mapped
    reader = nbt.Reader(value["val"])
    pairs, at, target = [], -1, 0
    while reader.pos < len(reader.data):
        at += 1 + reader.varint()
        target += reader.zigzag()
        if not 0 <= at < 2**31 - 1 or not -1 <= target < value["mappedSize"]:
            raise ValueError("Sparse mapping exceeds source-data bounds")
        pairs.append((at, target))
    source_size = max(value["size"], at + 1)
    runs, previous = [], -1
    for at, target in pairs:
        if at > previous + 1:
            runs.append((previous + 1, -1))
        runs.append((at, target))
        previous = at
    if previous + 1 < source_size:
        runs.append((previous + 1, -1))
    return runs, source_size, value["mappedSize"]


def map_runs(values):
    result = []
    for at, value in enumerate(values):
        if at and ((value == -1 and values[at - 1] == -1) or
                   (value >= 0 and values[at - 1] >= 0 and value == values[at - 1] + 1)):
            continue
        result.append((at, value))
    return result


def packet_names(path):
    java = re.sub(r"/\*.*?\*/|//[^\n]*", "", text(path), flags=re.S)
    body = java.split("{", 1)[1].split(";", 1)[0]
    entries = [part.strip() for part in body.split(",") if part.strip()]
    names = []
    for entry in entries:
        match = re.fullmatch(r"([A-Z][A-Z0-9_]*)(?:\((0x[0-9a-fA-F]+|\d+)\))?", entry)
        if match is None:
            raise ValueError(f"Unrecognized packet enum entry {entry!r}: {path}")
        name, explicit = match.groups()
        ident = int(explicit, 0) if explicit else len(names)
        if not len(names) <= ident <= 1024:
            raise ValueError(f"Out-of-order packet ID: {path}")
        names.extend([""] * (ident - len(names)))
        names.append(name)
    if not names or len(set(names) - {""}) != len([n for n in names if n]):
        raise ValueError(f"Invalid packet enum: {path}")
    return names


def export_18(base, destination):
    """Metadata ancestry and sound names for the first ViaVersion edge."""
    # Resolve from the checkout layout rather than a running Java classpath.
    checkout = next(parent for parent in base.parents if (parent / "api/src/main/java").is_dir())
    api = checkout / "api/src/main/java/com/viaversion/viaversion/api"
    source = base / "v1_8to1_9/data"
    paths = [api / "minecraft/entities/EntityTypes1_9.java", source / "EntityDataIndex1_9.java",
             source / "SoundEffectMappings1_9.java", source / "SoundCategories1_8.java",
             source / "PotionIdMappings1_9.java", source / "EntityIds1_8.java", source / "EffectIdMappings1_9.java"]
    lines = ["// SPDX-License-Identifier: GPL-3.0-or-later",
             "// Generated 1.8 -> 1.9 metadata, entity ancestry and named sounds."]
    for path in paths:
        lines.append(f"// {path.name} SHA256: {fingerprint(path)}")
    java = re.sub(r"/\*.*?\*/|//[^\n]*", "", text(paths[0]), flags=re.S)
    entity_body = re.search(r"enum EntityType[^\{]*\{(.*?);", java, re.S)[1]
    entities = []
    for name, args in re.findall(r"\b([A-Z][A-Z0-9_]*)\s*(?:\(([^()]*)\))?\s*[,;]", entity_body + ";"):
        values = [value.strip() for value in args.split(",") if value.strip()]
        ident = int(values[0]) if values and re.fullmatch(r"-?\d+", values[0]) else -1
        parent = values[-1] if values else ""
        entities.append(f"({json.dumps(name)},{json.dumps(parent)},{ident})")
    lines.append("pub const ENTITIES: &[(&str,&str,i32)] = &[" + ",".join(entities) + "];" )
    object_body = re.search(r"enum ObjectType[^\{]*\{(.*?);", java, re.S)[1]
    objects = []
    for args in re.findall(r"\w+\(([^()]*)\)", object_body):
        parts = [part.strip() for part in args.split(",")]
        ident = int(parts[0]); data = int(parts[1]) if len(parts) == 3 else -1
        objects.append(f"({ident},{data},{json.dumps(parts[-1].split('.')[-1])})")
    lines.append("pub const OBJECTS: &[(i32,i32,&str)] = &[" + ",".join(objects) + "];" )
    old_types = {name: index for index, name in enumerate(["BYTE", "SHORT", "INT", "FLOAT", "STRING", "ITEM", "BLOCK_POSITION", "ROTATIONS"])}
    new_types = {name: index for index, name in enumerate(["BYTE", "VAR_INT", "FLOAT", "STRING", "COMPONENT", "ITEM", "BOOLEAN", "ROTATIONS", "BLOCK_POSITION", "OPTIONAL_BLOCK_POSITION", "DIRECTION", "OPTIONAL_UUID", "OPTIONAL_BLOCK_STATE"])}
    body = re.sub(r"/\*.*?\*/|//[^\n]*", "", text(paths[1]), flags=re.S).split("{", 1)[1].split(";", 1)[0]
    entries = []
    for args in re.findall(r"\w+\(([^()]*)\)", body):
        parts = [part.strip() for part in args.split(",")]
        if len(parts) == 3:
            continue  # A new-only metadata field, no source value to convert.
        family, index = parts[:2]
        old_kind = old_types[parts[2].split(".")[-1]]
        new_index = parts[3] if len(parts) == 5 else index
        new_kind = -1 if parts[-1] == "null" else new_types[parts[-1].split(".")[-1]]
        entries.append(f"({json.dumps(family)},{index},{old_kind},{new_index},{new_kind})")
    lines.append("pub const METADATA: &[(&str,u8,u8,u8,i16)] = &[" + ",".join(entries) + "];" )
    categories = {name: int(ident) for name, ident in re.findall(r'\b([A-Z_]+)\("[^"]+",\s*(\d+)\)', text(paths[3]))}
    sounds = []
    for before, after, category in re.findall(r'\w+\("([^"]+)",\s*"([^"]+)",\s*SoundCategories1_8\.([A-Z_]+)(?:,\s*(?:true|false))?\)', text(paths[2])):
        sounds.append(f"({json.dumps(before)},{json.dumps(after)},{categories[category]})")
    lines.append("pub const SOUNDS: &[(&str,&str,i32)] = &[" + ",".join(sounds) + "];" )
    for symbol, path in [("POTIONS", paths[4]), ("EGGS", paths[5])]:
        entries = re.findall(r'register\((-?\d+),\s*"([^"]+)"\)', text(path))
        lines.append(f"pub const {symbol}: &[(i32,&str)] = &[" + ",".join(f"({ident},{json.dumps(name)})" for ident, name in entries) + "];" )
    entries = re.findall(r'addRewrite\((\d+),\s*(\d+)\)', text(paths[6]))
    lines.append("pub const EFFECTS: &[(i32,i32)] = &[" + ",".join(f"({before},{after})" for before, after in entries) + "];" )
    destination.joinpath("release_18_data.rs").write_text("\n".join(lines) + "\n", encoding="utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("viaversion", type=Path)
    parser.add_argument("vialegacy", type=Path)
    args = parser.parse_args()
    for checkout, revision in [(args.viaversion, VIA), (args.vialegacy, LEGACY)]:
        actual = subprocess.run(["git", "-C", str(checkout), "rev-parse", "HEAD"],
                                check=True, capture_output=True, text=True).stdout.strip()
        if actual != revision:
            raise ValueError(f"Expected pinned checkout {revision}, got {actual}")
    destination = Path(__file__).resolve().parents[1] / "mixins/viafabricplus"
    module = importlib.util.spec_from_file_location("mapping_export", Path(__file__).with_name("export-mappings.py"))
    nbt = importlib.util.module_from_spec(module)
    module.loader.exec_module(nbt)
    vv = args.viaversion / "common/src/main"
    vl = args.vialegacy / "src/main"
    base = vv / "java/com/viaversion/viaversion/protocols"
    legacy_base = vl / "java/net/raphimc/vialegacy/protocol/release"
    resources = vv / "resources/assets/viaversion/data"
    enums = {}
    for source in [base, legacy_base]:
        for path in sorted(source.rglob("*Packets*.java")):
            enums.setdefault(path.stem, path)
    output = ["// SPDX-License-Identifier: GPL-3.0-or-later",
              "// Generated by tools/export-release-chain.py; data is not a support claim.",
              f"// ViaVersion {VIA}; ViaLegacy {LEGACY}.",
              "use crate::{vfp_release_chain::{Step, Registry, RegistryNames}, vfp_wire::IdMap};"]
    emitted = set()

    def enum_symbol(name):
        symbol = re.sub(r"(?<=[a-z])(?=[A-Z])", "_", name).upper()
        if symbol not in emitted:
            path = enums[name]
            output.append(f"// {path.name} SHA256: {fingerprint(path)}")
            names = ",".join(json.dumps(value) for value in packet_names(path))
            output.append(f"const {symbol}: &[&str] = &[{names}];")
            emitted.add(symbol)
        return symbol

    steps = []
    last_types = None
    for index, (protocol, version, directory, mapping) in enumerate(RELEASES):
        next_protocol = RELEASES[index + 1][0] if index + 1 < len(RELEASES) else 761
        if directory is None:
            # 1.9.2 retained the 1.9.1 packet schemas; the next nontrivial edge
            # is 1.9.2 -> 1.9.3. Keep its separate handshake identity.
            types = [last_types[1], last_types[1], last_types[3], last_types[3]]
        else:
            folder = (legacy_base if protocol < 47 else base) / directory
            paths = sorted(folder.glob("Protocol*.java"))
            if len(paths) != 1:
                raise ValueError(f"Expected one protocol definition in {folder}")
            match = re.search(r"extends AbstractProtocol<([^>]+)>", text(paths[0]))
            types = [value.strip() for value in match[1].split(",")]
            output.append(f"// {paths[0].name} SHA256: {fingerprint(paths[0])}")
        last_types = types
        names = [enum_symbol(name) for name in types]
        maps = []
        if mapping:
            path = resources / f"mappings-{mapping}.nbt"
            output.append(f"// {path.name} SHA256: {fingerprint(path)}")
            for name, value in sorted(nbt.load(path).items()):
                if not isinstance(value, dict) or "mappedSize" not in value:
                    continue
                entries, source_size, size = mapping_runs(nbt, value)
                symbol = f"P{protocol}_" + name.replace("-", "_").upper()
                runs = ",".join(f"({at},{to})" for at, to in entries)
                output.append(f"const {symbol}: IdMap = IdMap {{size:{source_size},mapped_size:{size},runs:&[{runs}]}};")
                maps.append(f"Registry {{name:{json.dumps(name)},map:{symbol}}}")
        # Absence of a resource map means that registry changes require an
        # explicit adapter, not that the source and target registries match.
        steps.append((protocol, next_protocol, version, names, maps))
    reader = nbt.Reader((resources / "identifier-table.nbt").read_bytes())
    if reader.number("B") != 10:
        raise ValueError("Invalid global identifier table")
    reader.string()
    pool = reader.tag(10)
    aliases = [(340, "1.12"), (393, "1.13"), (401, "1.13.2"), (404, "1.13.2"),
               (477, "1.14"), (480, "1.14"), (485, "1.14"), (490, "1.14"), (498, "1.14"),
               (573, "1.15"), (575, "1.15"), (578, "1.15"), (735, "1.16"), (736, "1.16"),
               (751, "1.16.2"), (753, "1.16.2"), (754, "1.16.2"), (755, "1.17"), (756, "1.17"),
               (757, "1.18"), (758, "1.18"), (759, "1.19"), (760, "1.19"), (761, "1.19.3")]
    versions = {}
    resolved = {}
    for protocol, version in aliases:
        if version not in versions:
            symbol = "NAMES_" + version.replace(".", "_")
            entries = []
            path = resources / f"identifiers-{version}.nbt"
            output.append(f"// {path.name} SHA256: {fingerprint(path)}")
            identifiers = nbt.load(path)
            if version == "1.13":
                # Both protocol rewriters resolve numeric entity IDs using
                # EntityTypes1_13. Only the later resource stores their names.
                entity_path = resources / "identifiers-1.13.2.nbt"
                output.append(f"// Shared EntityTypes1_13 identifiers from {entity_path.name} SHA256: {fingerprint(entity_path)}")
                identifiers["entities"] = nbt.load(entity_path)["entities"]
            for name, value in sorted(identifiers.items()):
                if not isinstance(value, dict) or name not in pool:
                    continue
                indices, _ = nbt.decode(dict(value, mappedSize=len(pool[name])))
                if any(value < 0 for value in indices):
                    raise ValueError("Sparse registry identifier table")
                names = ",".join(json.dumps(pool[name][value], ensure_ascii=True) for value in indices)
                resolved[(version, name)] = [pool[name][value] for value in indices]
                entries.append(f"RegistryNames {{name:{json.dumps(name)},values:&[{names}]}}")
            output.append(f"const {symbol}: &[RegistryNames] = &[" + ",".join(entries) + "];" )
            versions[version] = symbol
    output.append("pub const IDENTIFIERS: &[(i32, &[RegistryNames])] = &[" +
                  ",".join(f"({protocol},{versions[version]})" for protocol, version in aliases) + "];" )
    alias = dict(aliases)
    block_sizes = {}
    for path in sorted(resources.glob("mappings-1.*to1.*.nbt")):
        before, after = path.stem.removeprefix("mappings-").split("to")
        mapping = nbt.load(path).get("blockstates")
        if mapping is not None:
            block_sizes[before] = mapping["size"]
            block_sizes[after] = mapping["mappedSize"]
    declarations = []
    for protocol, target, version, names, maps in steps:
        before, after = alias.get(protocol), alias.get(target)
        for registry in sorted(pool):
            if not before or not after or any(f'name:"{registry}"' in entry for entry in maps):
                continue
            old_values, new_values = resolved.get((before, registry)), resolved.get((after, registry))
            if old_values is None or new_values is None:
                continue
            # Some upstream mappings omit registries whose existing entries
            # keep their IDs when new entries are appended (1.15 blocks).
            # Match both identifier tables to verify every old entry survives.
            if old_values == new_values:
                entries = [(0, 0)]
            else:
                target_ids = {name: index for index, name in enumerate(new_values)}
                if len(target_ids) != len(new_values) or not all(name in target_ids for name in old_values):
                    continue
                entries = map_runs([target_ids[name] for name in old_values])
            runs = ','.join(f'({at},{to})' for at, to in entries)
            maps.append(f'Registry {{name:{json.dumps(registry)},map:IdMap {{size:{len(old_values)},mapped_size:{len(new_values)},runs:&[{runs}]}}}}')
        if before is not None and before == after and before in block_sizes:
            size = block_sizes[before]
            maps.append(f'Registry {{name:"blockstates",map:IdMap {{size:{size},mapped_size:{size},runs:&[(0,0)]}}}}')
        declarations.append(f"Step {{source:{protocol},target:{target},version:{json.dumps(version)},clientbound:{names[0]},next_clientbound:{names[1]},serverbound:{names[2]},next_serverbound:{names[3]},registries:&[{','.join(maps)}]}}")
    output.append("pub const STEPS: &[Step] = &[" + ",\n".join(declarations) + "];" )
    states_path = resources / "blockstates-1.13.nbt"
    reader = nbt.Reader(states_path.read_bytes())
    if reader.number("B") != 10:
        raise ValueError("Invalid legacy block-state table")
    reader.string()
    states = reader.tag(10)
    air, at = [], 0
    for block in states["blockstates"]:
        size = 1
        for property_index in block.get("properties", []):
            size *= len(states["properties"][property_index]["values"])
        if block["name"] in ["air", "cave_air", "void_air"]:
            air.extend(range(at, at + size))
        at += size
    air_versions = {"1.13": air}
    for before, after in [("1.13", "1.13.2"), ("1.13.2", "1.14"), ("1.14", "1.15"),
                          ("1.15", "1.16"), ("1.16", "1.16.2"), ("1.16.2", "1.17"),
                          ("1.17", "1.18"), ("1.18", "1.19"), ("1.19", "1.19.3")]:
        values, _ = nbt.decode(nbt.load(resources / f"mappings-{before}to{after}.nbt")["blockstates"])
        air_versions[after] = [values[index] for index in air_versions[before]]
    output.append(f"// Air states derived from {states_path.name} SHA256: {fingerprint(states_path)}")
    output.append("pub const AIR_STATES: &[(i32, &[i32])] = &[" +
                  ",".join(f"({protocol},&[{','.join(map(str, air_versions[version]))}])" for protocol, version in aliases if version in air_versions) + "];" )
    entities_path = args.viaversion / "api/src/main/java/com/viaversion/viaversion/api/minecraft/entities/EntityTypes1_14.java"
    entity_java = re.sub(r"/\*.*?\*/|//[^\n]*", "", text(entities_path), flags=re.S).split("{", 1)[1].split(";", 1)[0]
    parents = {name: args.split(",")[0].strip() for name, args in re.findall(r"\b([A-Z][A-Z0-9_]*)\(([^()]*)\)", entity_java)}
    family_names = ["LIVING_ENTITY", "MOB", "ZOMBIE", "ABSTRACT_SKELETON",
                    "ABSTRACT_ILLAGER", "ABSTRACT_RAIDER", "ABSTRACT_ARROW", "ABSTRACT_MINECART"]
    families = {}
    for name in resolved[("1.14", "entities")]:
        current, visited = name.upper(), set()
        if current not in parents:
            raise ValueError(f"Unknown 1.14 entity hierarchy entry: {name}")
        while current not in visited:
            visited.add(current)
            current = parents.get(current, "null")
        families[name] = sum(1 << i for i, parent in enumerate(family_names) if parent in visited)
    output.append(f"// Living entity families from {entities_path.name} SHA256: {fingerprint(entities_path)}")
    output.append("pub const LIVING_114: &[&str] = &[" + ",".join(json.dumps(name) for name, flags in families.items() if flags & 1) + "];" )
    output.append("pub const FAMILIES_114: &[(&str,u8)] = &[" + ",".join(f"({json.dumps(name)},{flags})" for name, flags in families.items()) + "];")
    objects_path = entities_path.with_name("EntityTypes1_13.java")
    java = re.sub(r"/\*.*?\*/|//[^\n]*", "", text(objects_path), flags=re.S)
    entity_enum = java.split("enum EntityType ", 1)[1].split("{", 1)[1].split(";", 1)[0]
    object_enum = java.split("enum ObjectType ", 1)[1].split("{", 1)[1].split(";", 1)[0]
    entity_ids = dict(re.findall(r"\b([A-Z][A-Z0-9_]*)\((\d+),", entity_enum))
    objects = re.findall(r"\b[A-Z][A-Z0-9_]*\((\d+),\s*(?:(\d+),\s*)?EntityType\.([A-Z][A-Z0-9_]*)\)", object_enum)
    output.append(f"// Object IDs from {objects_path.name} SHA256: {fingerprint(objects_path)}")
    output.append("pub const OBJECTS_113: &[(i32,i32,i32)] = &[" +
                  ",".join(f"({kind},{data or -1},{entity_ids[name]})" for kind, data, name in objects) + "];")
    names_base = base / "v1_10to1_11/data"
    for symbol, filename in [("ENTITY_NAMES_111", "EntityMappings1_11.java"),
                             ("BLOCK_ENTITY_NAMES_111", "BlockEntityMappings1_11.java")]:
        path = names_base / filename
        pairs = re.findall(r'rewrite\("([^"]+)",\s*"([^"]+)"\);', text(path))
        output.append(f"// {filename} SHA256: {fingerprint(path)}")
        output.append(f"pub const {symbol}: &[(&str,&str)] = &[" +
                      ",".join(f"({json.dumps(old)},{json.dumps('minecraft:' + new)})" for old, new in pairs) + "];")
    path = names_base / "PotionColorMappings1_11.java"
    potions = re.findall(r"addRewrite\((\d+),\s*(\d+),\s*(true|false)\);", text(path))
    output.append(f"// {path.name} SHA256: {fingerprint(path)}")
    output.append("pub const POTION_COLORS_111: &[(i32,i32,bool)] = &[" +
                  ",".join(f"({index},{color},{instant})" for index, color, instant in potions) + "];")
    flatten_base = base / "v1_12_2to1_13"
    path = flatten_base / "data/EntityIdMappings1_13.java"
    pairs = re.findall(r"registerEntity\((\d+),\s*(\d+)\);", text(path))
    output.append(f"// {path.name} SHA256: {fingerprint(path)}")
    output.append("pub const ENTITIES_112: &[(i32,i32)] = &[" +
                  ",".join(f"({old},{new})" for old, new in pairs) + "];")
    path = flatten_base / "data/ParticleIdMappings1_13.java"
    particles = re.findall(r"^\s*add\((-?\d+)[),]", text(path), re.M)
    output.append(f"// {path.name} SHA256: {fingerprint(path)}")
    output.append("pub const PARTICLES_112: &[i32] = &[" + ",".join(particles) + "];")
    path = flatten_base / "provider/PaintingProvider.java"
    paintings = re.findall(r'add\("([^"]+)"\);', text(path))
    output.append(f"// {path.name} SHA256: {fingerprint(path)}")
    output.append("pub const PAINTINGS_112: &[&str] = &[" + ",".join(map(json.dumps, paintings)) + "];")
    path = flatten_base / "data/SpawnEggMappings1_13.java"
    eggs = re.findall(r'registerSpawnEgg\("([^"]+)"\);', text(path))
    output.append(f"// {path.name} SHA256: {fingerprint(path)}")
    output.append("pub const EGGS_112: &[&str] = &[" + ",".join(json.dumps("minecraft:" + egg) for egg in eggs) + "];")
    path = resources / "channelmappings-1.13.json"
    channels = json.loads(text(path))
    output.append(f"// {path.name} SHA256: {fingerprint(path)}")
    output.append("pub const CHANNELS_112: &[(&str,&str)] = &[" +
                  ",".join(f"({json.dumps(old)},{json.dumps(new)})" for old, new in channels.items()) + "];")
    path = flatten_base / "data/StatisticMappings1_13.java"
    statistics = re.findall(r'CUSTOM_STATS\.put\("([^"]+)",\s*(\d+)\);', text(path))
    output.append(f"// {path.name} SHA256: {fingerprint(path)}")
    output.append("pub const STATS_112: &[(&str,i32)] = &[" +
                  ",".join(f"({json.dumps(name)},{value})" for name, value in statistics) + "];")
    path = resources / "blockIds1.12to1.13.json"
    aliases_112 = json.loads(text(path))
    output.append(f"// Block identifier aliases from {path.name} SHA256: {fingerprint(path)}")
    output.append("pub const BLOCK_ALIASES_112: &[(&str,&[&str])] = &[" +
                  ",".join(f"({json.dumps(name)},&[{','.join(map(json.dumps, ids))}])"
                           for name, ids in sorted(aliases_112.items())) + "];")
    path = resources / "mappings-1.12to1.13.nbt"
    enchants = nbt.load(path)["legacy_enchantments"]
    output.append(f"// Legacy enchantments from {path.name} SHA256: {fingerprint(path)}")
    output.append("pub const ENCHANTS_112: &[(i16,&str)] = &[" +
                  ",".join(f"({index},{json.dumps(name)})" for index, name in sorted(enchants.items())) + "];")
    path = entities_path.with_name("EntityTypes1_8.java")
    java = re.sub(r"/\*.*?\*/|//[^\n]*", "", text(path), flags=re.S)
    entity_enum = java.split("enum EntityType ", 1)[1].split("{", 1)[1].split(";", 1)[0]
    object_enum = java.split("enum ObjectType ", 1)[1].split("{", 1)[1].split(";", 1)[0]
    entities = re.findall(r"\b([A-Z][A-Z0-9_]*)\((?:(-?\d+),\s*)?([A-Z_]+|null)\)", entity_enum)
    objects = re.findall(r"\b[A-Z][A-Z0-9_]*\((\d+),\s*(?:(\d+),\s*)?EntityType\.([A-Z][A-Z0-9_]*)\)", object_enum)
    output.append(f"// 1.7 translation entity families from {path.name} SHA256: {fingerprint(path)}")
    output.append("pub const ENTITIES_17: &[(&str,&str,i32)] = &[" +
                  ",".join(f"({json.dumps(kind)},{json.dumps(parent if parent != 'null' else '')},{ident or -1})"
                           for kind, ident, parent in entities) + "];")
    output.append("pub const OBJECTS_17: &[(i32,i32,&str)] = &[" +
                  ",".join(f"({kind},{data or -1},{json.dumps(name)})" for kind, data, name in objects) + "];")
    path = legacy_base / "r1_7_6_10tor1_8/data/EntityDataIndex1_7_6.java"
    metadata = []
    kinds = ["BYTE", "SHORT", "INT", "FLOAT", "STRING", "ITEM", "BLOCK_POSITION", "ROTATIONS"]
    java = re.sub(r"/\*.*?\*/|//[^\n]*", "", text(path), flags=re.S).split("enum EntityDataIndex1_7_6", 1)[1]
    for args in re.findall(r"\w+\(([^()]*)\)", java.split(";", 1)[0]):
        family, index, source_type, *target = [part.strip() for part in args.split(",")]
        if source_type == "null":
            continue
        metadata.append(f"({json.dumps(family.rsplit('.', 1)[1])},{index},{kinds.index(source_type.rsplit('.', 1)[1])},"
                        f"{target[0] if len(target) == 2 else index},{kinds.index(target[-1].rsplit('.', 1)[1])})")
    output.append(f"// 1.7 metadata from {path.name} SHA256: {fingerprint(path)}")
    output.append("pub const METADATA_17: &[(&str,u8,i32,u8,i32)] = &[" + ",".join(metadata) + "];")
    destination.joinpath("release_chain_data.rs").write_text("\n".join(output) + "\n", encoding="utf-8")
    bundles = ["// SPDX-License-Identifier: GPL-3.0-or-later",
               "// Generated upstream registry resources, parsed by Rust without Java."]
    export_flatten_resources(nbt, resources, bundles)
    for name in ["dimension-registry-1.16.2.nbt", "chat-registry-1.19.nbt", "chat-registry-1.19.1.nbt", "chat-types-1.19.nbt", "heightmap-1.14.nbt"]:
        path = resources / name
        symbol = name.removesuffix(".nbt").replace("-", "_").replace(".", "_").upper()
        bundles.append(f"// {name} SHA256: {fingerprint(path)}")
        bundles.append(f"pub const {symbol}: &[u8] = &[" + ",".join(map(str, path.read_bytes())) + "];" )
        if name == "heightmap-1.14.nbt":
            reader = nbt.Reader(path.read_bytes()); reader.number("B"); reader.string()
            ranges = nbt.Reader(reader.tag(10)["motionBlocking"])
            values, previous = [], 0
            while ranges.pos < len(ranges.data):
                start = previous + ranges.varint(); end = start + ranges.varint()
                values.extend(range(start, end + 1)); previous = end + 1
            bundles.append("pub const MOTION_BLOCKING_114: &[i32] = &[" + ",".join(map(str, values)) + "];" )
    destination.joinpath("release_chain_resources.rs").write_text("\n".join(bundles) + "\n", encoding="utf-8")
    export_connections(nbt, resources, destination)
    export_18(base, destination)
    print("Wrote Rust release-chain descriptors and bundled registry resources. No build or tests run.")


def export_connections(nbt, resources, destination):
    states_path = resources / "blockstates-1.13.nbt"
    profiles_path = resources / "blockConnections.nbt"
    def read_compound(path):
        reader = nbt.Reader(path.read_bytes())
        if reader.number("B") != 10:
            raise ValueError(f"Expected an NBT compound in {path}")
        reader.string()
        return reader.tag(10)

    states = read_compound(states_path)
    connections = read_compound(profiles_path)
    rows = ["// SPDX-License-Identifier: GPL-3.0-or-later",
            "// Generated by tools/export-release-chain.py.",
            f"// {states_path.name} SHA256: {fingerprint(states_path)}",
            f"// {profiles_path.name} SHA256: {fingerprint(profiles_path)}",
            "use crate::vfp_release_connections::{Block, Property};"]
    for index, prop in enumerate(states["properties"]):
        values = ",".join(map(json.dumps, prop["values"]))
        rows.append(f'const P{index}: Property = Property {{name:{json.dumps(prop["name"])},values:&[{values}]}};')
    rows.append("pub const BLOCKS: &[Block] = &[")
    count = 0
    for block in states["blockstates"]:
        properties = block.get("properties", [])
        fields = ",".join(f"P{index}" for index in properties)
        rows.append(f'Block {{name:{json.dumps(block["name"])},start:{count},properties:&[{fields}]}},')
        variants = 1
        for index in properties:
            variants *= len(states["properties"][index]["values"])
        count += variants
    rows.append("];")

    def ranges(data):
        reader = nbt.Reader(data)
        end = 0
        while reader.pos < len(reader.data):
            start = end + reader.varint()
            end = start + reader.varint() + 1
            yield from range(start, end)

    ids = [0] * count
    profiles = [[0] * 6]
    for profile in connections["profiles"]:
        for state in ranges(profile["ids"]):
            ids[state] = len(profiles)
        masks = [profile.get(str(index), 0) & 15 for index in range(5)]
        masks.append(int("5" in profile))
        profiles.append(masks)
    if len(profiles) > 256:
        raise ValueError("1.13 block connection profile index exceeds u8")
    rows.append("pub const PROFILE_IDS: &[u8] = &[" + ",".join(map(str, ids)) + "];")
    rows.append("pub const PROFILES: &[[u8;6]] = &[" +
                ",".join("[" + ",".join(map(str, row)) + "]" for row in profiles) + "];")
    rows.append("pub const OCCLUDING: &[u16] = &[" +
                ",".join(map(str, ranges(connections["occluding-states"]))) + "];")
    destination.joinpath("release_connection_data.rs").write_text("\n".join(rows) + "\n", encoding="utf-8")


def export_flatten_resources(nbt, resources, bundles):
    import struct

    def varint(value):
        value &= 0xFFFFFFFF
        result = bytearray()
        while value >= 128:
            result.append((value & 127) | 128)
            value >>= 7
        result.append(value)
        return result

    def string(value):
        data = value.encode("utf-8")
        return varint(len(data)) + data

    def stack(item, count=1):
        return struct.pack(">hbB", item, count, 0)

    def ingredient(items):
        if items is None:
            return varint(0)
        if isinstance(items, int):
            items = [items]
        return varint(len(items)) + b"".join(stack(item) for item in items)

    def emit(name, path, data):
        bundles.append(f"// {path.name} SHA256: {fingerprint(path)}")
        bundles.append(f"pub const {name}: &[u8] = &[" + ",".join(map(str, data)) + "];")

    path = resources / "itemrecipes1_12_2to1_13.json"
    definitions = json.loads(text(path))
    data = varint(sum(map(len, definitions.values())))
    for recipe_id, recipe in definitions["shapeless"].items():
        ingredients = recipe["ingredients"]
        data += string("viaversion:legacy/" + recipe_id) + string("crafting_shapeless")
        data += string(recipe.get("group", "")) + varint(len(ingredients))
        for slot in ingredients:
            data += ingredient(slot)
        data += stack(recipe["result"], recipe.get("count", 1))
    for recipe_id, recipe in definitions["shaped"].items():
        grid = recipe["grid"]
        data += string("viaversion:legacy/" + recipe_id) + string("crafting_shaped")
        data += varint(len(grid[0])) + varint(len(grid)) + string(recipe.get("group", ""))
        for row in grid:
            for slot in row:
                data += ingredient(slot)
        data += stack(recipe["result"], recipe.get("count", 1))
    for recipe_id, recipe in definitions["smelting"].items():
        data += string(recipe_id) + string("smelting") + string(recipe.get("group", ""))
        data += ingredient(recipe["ingredient"]) + stack(recipe["result"], recipe.get("count", 1))
        data += struct.pack(">f", recipe["experience"]) + varint(200)
    for recipe_id, kind in definitions["special"].items():
        data += string(recipe_id) + string("crafting_special_" + kind)
    emit("RECIPES_113", path, data)

    path = resources / "mappings-1.12to1.13.nbt"
    mappings = nbt.load(path)
    data = bytearray()
    for key in ("block_tags", "item_tags", "fluid_tags"):
        tags = mappings[key]
        data += varint(len(tags))
        for name, ids in sorted(tags.items()):
            data += string(name if ":" in name else "minecraft:" + name)
            data += varint(len(ids))
            for value in ids:
                data += varint(value)
    emit("TAGS_113", path, data)


if __name__ == "__main__":
    main()
