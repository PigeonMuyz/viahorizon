#!/usr/bin/env python3
"""Verify and prepare ViaHorizon's Horizon registry submission without submitting it."""
import argparse
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[1]
SOURCE = "https://github.com/hhhhsffd/viahorizon"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--checker", required=True, type=Path, help="cargo-horizon-mod executable")
    args = parser.parse_args()
    checker = args.checker.resolve()
    compat = json.loads((ROOT / "compatibility.json").read_text(encoding="utf-8"))
    manifest = tomllib.loads((ROOT / "mixins/viahorizon/horizon-mixin.toml").read_text(encoding="utf-8"))
    page = tomllib.loads((ROOT / "horizon-registry.toml").read_text(encoding="utf-8"))
    package = ROOT / "dist" / f"viahorizon-{manifest['mod']['version']}.hnmod"
    spec = importlib.util.spec_from_file_location("native", ROOT / "tools/native.py")
    native = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(native)
    receipt = json.loads((ROOT / "target/native-check.json").read_text(encoding="utf-8"))
    if receipt != {"kind": "check", "horizon": compat["horizon_revision"], "bundle_sha256": native.bundle_digest()}:
        raise SystemExit("Native check is stale; run tools/native.py check against the pinned Nightly first")
    if not page["summary"] or "\n" in page["summary"]:
        raise SystemExit("Registry summary must be one nonempty line")
    # Package twice from the same source to prove byte-for-byte reproducibility.
    command = [sys.executable, str(ROOT / "tools/native.py"), "package"]
    subprocess.run(command, check=True, stdout=subprocess.DEVNULL)
    first = package.read_bytes()
    subprocess.run(command, check=True, stdout=subprocess.DEVNULL)
    if first != package.read_bytes():
        raise SystemExit("Package is not reproducible")
    sha = hashlib.sha256(first).hexdigest()
    with zipfile.ZipFile(package) as archive:
        header = json.loads(archive.read("horizon-native.json"))
        bundle = f"mixins/{header['id']}/"
        assert archive.read(bundle + "LICENSE") == (ROOT / "LICENSE").read_bytes()
        assert archive.read(bundle + "NOTICE.md") == (ROOT / "THIRD_PARTY_NOTICES.md").read_bytes()
        assert "hooks" not in header, "Internal hooks must keep exact revision compatibility"
    raw = subprocess.check_output([str(checker), "check", str(package), "--kind", "native-mod", "--json"], text=True, encoding="utf-8")
    response = json.loads(raw)
    if response.get("ok") is not True:
        raise SystemExit(f"Package check failed: {response}")
    check = response["report"]
    expected = {"kind": "native-mod", "id": header["id"], "version": header["version"], "game": header["game"], "sha256": sha}
    for key, value in expected.items():
        if check.get(key) != value:
            raise SystemExit(f"Checker {key} mismatch: {check.get(key)!r} != {value!r}")
    entry = {"id": header["id"], "kind": "native-mod", **page, "source": SOURCE,
             "version": header["version"], "game": header["game"],
             "changelog": "Default new configurations to Auto server version detection; preserve saved targets and per-server overrides."}
    output = ROOT / "dist" / "publishing"
    output.mkdir(exist_ok=True)
    (output / "submission.json").write_text(json.dumps(entry, indent=2) + "\n", encoding="utf-8")
    (output / "package-check.json").write_text(json.dumps(check, indent=2) + "\n", encoding="utf-8")
    quote = lambda value: json.dumps(value, ensure_ascii=True)
    lines = [f"{key} = {quote(entry[key])}" for key in ("id", "kind", "name", "summary", "description", "authors", "license", "source")]
    lines += ['owners = ["hhhhsffd"]', "", "[[versions]]", f"version = {quote(header['version'])}",
              f"published = {quote(datetime.now(timezone.utc).date().isoformat())}", f"game = {quote(header['game'])}",
              f"file = {quote(package.name)}", f"sha256 = {quote(sha)}", f"changelog = {quote(entry['changelog'])}"]
    (output / f"{header['id']}.toml").write_text("\n".join(lines) + "\n", encoding="utf-8")
    files = output / "files"
    files.mkdir(exist_ok=True)
    shutil.copyfile(package, files / sha)
    # Run the same entry/file and index validation as registry CI, using only
    # this submission so unrelated projects need not be downloaded.
    trial = ROOT / "target" / "publishing" / "entry-check" / header["id"]
    base, head = trial / "base", trial / "head"
    for directory in (base / "mods", head / "mods"):
        directory.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(output / f"{header['id']}.toml", head / "mods" / f"{header['id']}.toml")
    for command in (
        ["registry", "review", str(base), str(head), "--author", "hhhhsffd", "--json"],
        ["registry", "check", str(base), str(head), "--files", str(files)],
        ["registry", "build", str(head), "--files", str(files), "--out", str(output / "index-preview.json"),
         "--generated", datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")],
    ):
        subprocess.run([str(checker), *command], check=True)
    result = {"package": package.name, "sha256": sha, "size": len(first), "reproducible": True,
              "package_check": check, "native_check": receipt, "signed": False,
              "registry_entry_check": "passed", "registry_index_build": "passed",
              "submission_status": "prepared; not submitted"}
    (output / "readiness.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
