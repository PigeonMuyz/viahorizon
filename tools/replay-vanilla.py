#!/usr/bin/env python3
"""Replay a private vanilla transcript against the currently transformed protocol."""
import argparse
import os
from pathlib import Path
import shutil
import subprocess

root=Path(__file__).resolve().parents[1]
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('release')
parser.add_argument('trace',type=Path)
parser.add_argument('--profile',default='dev')
args=parser.parse_args()
stage=root/'target/horizon-stage'
destination=stage/'crates/horizon-protocol/tests/vfp_vanilla_replay.rs'
destination.parent.mkdir(exist_ok=True)
source=root/'tests/horizon_vanilla_replay.rs'
if not destination.exists() or destination.read_bytes()!=source.read_bytes():
    shutil.copyfile(source,destination)
env=os.environ.copy()
env.update(VFP_REPLAY_RELEASE=args.release,VFP_REPLAY_TRACE=str(args.trace.resolve()))
subprocess.run(['cargo','test','--locked','--profile',args.profile,'-p','horizon-protocol',
                '--test','vfp_vanilla_replay','--','--ignored','--nocapture'],
               cwd=stage,env=env,check=True)
