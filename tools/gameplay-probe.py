#!/usr/bin/env python3
"""Build a quarantined real-game probe; never promotes production capabilities.

The copied bundle changes only target admission and the version label. All
packet translators come unchanged from the working source. Its executable and
provenance live under target/gameplay-probe, outside dist and launcher instances.
Run tools/native.py check/build to restore the production stage afterwards.
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tomllib
import uuid

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--horizon', type=Path, required=True)
    parser.add_argument('--mixins', type=Path, required=True)
    parser.add_argument('--replay', type=Path, action='append', default=[],
                        help='Replay a previous private vanilla run before compiling the game')
    args = parser.parse_args()
    pins = json.loads((ROOT/'compatibility.json').read_text(encoding='utf-8'))
    revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=args.horizon, text=True).strip()
    if revision != pins['horizon_revision']:
        raise SystemExit('Wrong Horizon source revision')
    if subprocess.check_output(['git', 'status', '--porcelain', '--untracked-files=no'], cwd=args.horizon):
        raise SystemExit('Horizon checkout must be pristine')
    spec = importlib.util.spec_from_file_location('vfp_native', ROOT/'tools/native.py')
    native = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(native)
    base_digest = native.bundle_digest()
    workspace = ROOT/'target'/'gameplay-probe'
    workspace.mkdir(exist_ok=True)
    mods = workspace/('mods-'+uuid.uuid4().hex)
    bundle = mods/'viafabricplus'
    shutil.copytree(ROOT/'mixins'/'viafabricplus', bundle)
    core = bundle/'core.rs'
    text = core.read_text(encoding='utf-8')
    version = tomllib.loads((bundle/'horizon-mixin.toml').read_text(encoding='utf-8'))['mod']['version']
    old = f'pub const VERSION: &str = "{version}";'
    assert text.count(old) == 1
    text = text.replace(old, f'pub const VERSION: &str = "{version} gameplay probe";')
    text = re.sub(r'(legacy: false,\s+stage: Stage::)(Planned|ConfigurationPreview)',
                  r'\g<1>Gameplay', text)
    core.write_text(text, encoding='utf-8')
    stage = ROOT/'target'/'horizon-stage'
    fresh = ROOT/'target'/('horizon-stage-new-'+uuid.uuid4().hex)
    subprocess.run([str(args.mixins.resolve()), 'apply', '--workspace', str(args.horizon.resolve()),
                    '--mods', str(mods), '--out', str(fresh)], check=True)
    if stage.exists():
        native.sync_stage(fresh, stage)
    else:
        fresh.rename(stage)
    # Invalidate production proof scripts before any compiler command can fail.
    stamp = ROOT/'target'/'native-stage.json'
    stamp.write_text(json.dumps({'mode':'gameplay-probe', 'bundle_sha256':None,
                                'base_bundle_sha256':base_digest})+'\n', encoding='utf-8')
    for run in args.replay:
        result = json.loads((run/'result.json').read_text(encoding='utf-8'))
        subprocess.run([sys.executable, str(ROOT/'tools/replay-vanilla.py'),
                        result['release'], str((run/'wire.jsonl').resolve()), '--profile', 'fast'],
                       cwd=ROOT, check=True)
    subprocess.run(['cargo', 'build', '--locked', '-p', 'horizon-client', '--bin', 'horizon-client',
                    '--bin', 'horizon-ctl', '--profile', 'fast'], cwd=stage, check=True)
    if native.bundle_digest() != base_digest:
        raise SystemExit('Working source changed while building; rerun the gameplay probe')
    executables = []
    for name in ('horizon-client.exe', 'horizon-ctl.exe'):
        source = stage/'target'/'fast'/name
        destination = workspace/name
        shutil.copy2(source, destination)
        executables.append({'file':name,'sha256':hashlib.sha256(destination.read_bytes()).hexdigest()})
    provenance = {'kind':'gameplay-probe-not-a-release','horizon':revision,
                  'base_bundle_sha256':base_digest, 'bundle':str(bundle), 'executables':executables}
    (workspace/'build.json').write_text(json.dumps(provenance,indent=2)+'\n', encoding='utf-8')
    print(json.dumps(provenance, indent=2), flush=True)


if __name__ == '__main__':
    main()
