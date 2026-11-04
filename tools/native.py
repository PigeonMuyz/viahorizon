#!/usr/bin/env python3
"""Package or compile the mod using Horizon's actual source transformer.

No upstream source tree is modified. A full check needs Rust, a C toolchain,
the pinned Horizon source, and the pinned horizon-mixins executable.
"""
import argparse
import filecmp
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tomllib
import uuid
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def bundle_digest():
    digest = hashlib.sha256()
    for path in sorted((ROOT / "mixins").rglob("*")):
        if path.is_file() and path.suffix in (".rs", ".toml"):
            digest.update(path.relative_to(ROOT).as_posix().encode())
            digest.update(b"\0")
            digest.update(path.read_bytes())
    return digest.hexdigest()


def sync_stage(fresh, stage):
    """Windows mixins cannot replace a stage atomically; preserve Cargo's cache."""
    allowed = (ROOT / "target").resolve()
    for path in (fresh, stage):
        resolved = path.resolve()
        if resolved == allowed or not resolved.is_relative_to(allowed):
            raise SystemExit(f"Refusing to change a path outside the build directory: {resolved}")
    if not (stage / ".horizon-mixins-stage.json").is_file():
        raise SystemExit("Existing stage has no horizon-mixins ownership marker")
    incoming = set()
    for folder, dirs, files in os.walk(fresh):
        relative = Path(folder).relative_to(fresh)
        (stage / relative).mkdir(parents=True, exist_ok=True)
        for name in files:
            source = Path(folder) / name
            destination = stage / relative / name
            incoming.add(relative / name)
            if not destination.is_file() or not filecmp.cmp(source, destination, shallow=False):
                shutil.copy2(source, destination)
    for folder, dirs, files in os.walk(stage):
        relative = Path(folder).relative_to(stage)
        if relative == Path("."):
            dirs[:] = [d for d in dirs if d != "target"]
        for name in files:
            if relative / name not in incoming:
                (Path(folder) / name).unlink()
    # Both absolute paths were checked against our private build directory above.
    shutil.rmtree(fresh)


def package():
    compat = json.loads((ROOT / "compatibility.json").read_text())
    bundle = ROOT / "mixins" / "viafabricplus"
    manifest = tomllib.loads((bundle / "horizon-mixin.toml").read_text())
    assert manifest["compatibility"]["horizon_revision"] == compat["horizon_revision"]
    native = {"schema": 1, "id": manifest["mod"]["id"], "version": manifest["mod"]["version"],
              "game": "26.3", "horizon": compat["horizon_revision"], "mixins": compat["horizon_mixins_revision"]}
    # No `hooks`: this bundle targets internals and MUST remain pinned.
    files = {"horizon-native.json": (json.dumps(native, indent=2, sort_keys=True) + "\n").encode()}
    for source in sorted(bundle.iterdir()):
        if source.suffix in (".rs", ".toml"):
            files[f"mixins/viafabricplus/{source.name}"] = source.read_bytes()
    out = ROOT / "dist" / f"viafabricplus-{native['version']}.hnmod"
    out.parent.mkdir(exist_ok=True)
    with zipfile.ZipFile(out, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for name, data in sorted(files.items()):
            info = zipfile.ZipInfo(name, (2026, 10, 30, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o100644 << 16
            archive.writestr(info, data)
    sha = hashlib.sha256(out.read_bytes()).hexdigest()
    out.with_suffix(".hnmod.sha256").write_text(f"{sha}  {out.name}\n")
    print(json.dumps({"artifact": str(out), "sha256": sha}, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["package", "plan", "check", "build"])
    parser.add_argument("--horizon", type=Path)
    parser.add_argument("--mixins", type=Path)
    args = parser.parse_args()
    if args.action == "package":
        package()
        return
    if not args.horizon or not args.mixins:
        parser.error("--horizon SOURCE and --mixins EXECUTABLE are required")
    expected = json.loads((ROOT / "compatibility.json").read_text())["horizon_revision"]
    actual = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=args.horizon, text=True).strip()
    if actual != expected:
        parser.error(f"Horizon must be pinned to {expected}, found {actual}")
    dirty = subprocess.check_output(["git", "status", "--porcelain", "--untracked-files=no"], cwd=args.horizon, text=True)
    if dirty.strip():
        parser.error("Horizon source has tracked modifications; use a pristine checkout")
    target = ROOT / "target"
    target.mkdir(exist_ok=True)
    stage = target / "horizon-stage"
    out = target / "mixin-plan.json" if args.action == "plan" else stage
    if args.action != "plan" and os.name == "nt" and stage.exists():
        out = target / f"horizon-stage-new-{uuid.uuid4().hex}"
    digest = bundle_digest()
    subprocess.run([str(args.mixins.resolve()), "plan" if args.action == "plan" else "apply",
                    "--workspace", str(args.horizon.resolve()), "--mods", str(ROOT / "mixins"), "--out", str(out)], check=True)
    if args.action != "plan" and out != stage:
        sync_stage(out, stage)
    if args.action != "plan":
        if bundle_digest() != digest:
            raise SystemExit("Mod sources changed during staging; rerun the native check")
        (target / "native-stage.json").write_text(json.dumps({"bundle_sha256": digest}) + "\n")
    if args.action in ("check", "build"):
        command = ["cargo", args.action, "--locked", "-p", "horizon-client", "--bin", "horizon-client"]
        if args.action == "build":
            command.extend(["--bin", "horizon-ctl", "--profile", "fast"])
        subprocess.run(command, cwd=stage, check=True)
        if args.action == "build":
            if bundle_digest() != digest:
                raise SystemExit("Mod sources changed while compiling; rebuild before validating")
            binaries = []
            for name in ("horizon-client.exe", "horizon-ctl.exe") if os.name == "nt" else ("horizon-client", "horizon-ctl"):
                path = stage / "target" / "fast" / name
                binaries.append({"path": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()})
            (target / "native-game-build.json").write_text(json.dumps({
                "kind": "production", "horizon": expected, "bundle_sha256": digest, "binaries": binaries,
            }, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
