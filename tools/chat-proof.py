#!/usr/bin/env python3
"""Run certificate/provider/acknowledgement checks against the transformed host."""
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess

root=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('native',root/'tools/native.py')
native=importlib.util.module_from_spec(spec);spec.loader.exec_module(native)
stamp=json.loads((root/'target/native-stage.json').read_text(encoding='utf-8'))
assert stamp['bundle_sha256']==native.bundle_digest(),'Restage the current source first'
stage=root/'target/horizon-stage'
tests=stage/'crates/horizon-client-render/tests'
tests.mkdir(exist_ok=True);(tests/'data').mkdir(exist_ok=True)
shutil.copyfile(root/'tests/horizon_signed_chat.rs',tests/'vfp_signed_chat.rs')
for name in ('chat-key.json','chat-services.der'):
    shutil.copyfile(root/'tests/data'/name,tests/'data'/name)
subprocess.run(['cargo','test','--locked','-p','horizon-client-render','--test','vfp_signed_chat','--','--test-threads=1'],cwd=stage,check=True)
