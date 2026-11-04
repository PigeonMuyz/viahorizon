#!/usr/bin/env python3
"""Run the real-server gameplay checks for protocol-family representatives."""
import argparse
import concurrent.futures
import json
from pathlib import Path
import subprocess
import sys
import uuid

ROOT=Path(__file__).resolve().parents[1]
VERSIONS=['1.19.3','1.19.4','1.20.1','1.20.2','1.20.4','1.20.6','1.21.1','1.21.3',
          '1.21.4','1.21.5','1.21.6','1.21.8','1.21.10','1.21.11','26.1.2','26.2','26.3']


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--resources',type=Path,required=True)
    parser.add_argument('--workers',type=int,choices=[1,2],default=2)
    parser.add_argument('--join-only',action='store_true',help='Check world entry and chunk decoding only')
    parser.add_argument('--client',type=Path)
    parser.add_argument('--ctl',type=Path)
    parser.add_argument('versions',nargs='*')
    args=parser.parse_args()
    versions=args.versions or VERSIONS
    out=ROOT/'target'/('vanilla-matrix-'+uuid.uuid4().hex[:8]);out.mkdir()
    print(json.dumps({'matrix':str(out),'versions':versions}),flush=True)
    def run(version):
        log=out/(version+'.log')
        with log.open('wb') as output:
            command=[sys.executable,str(ROOT/'tools/vanilla-gameplay.py'),version,
                     '--resources',str(args.resources),'--timeout','240']
            if args.join_only: command.append('--join-only')
            if args.client: command.extend(['--client',str(args.client)])
            if args.ctl: command.extend(['--ctl',str(args.ctl)])
            process=subprocess.run(command,cwd=ROOT,
                                   stdout=output,stderr=subprocess.STDOUT)
        result=None
        for line in log.read_text(encoding='utf-8',errors='replace').splitlines():
            if line.startswith('{"run":'):
                path=Path(json.loads(line)['run'])/'result.json'
                if path.is_file():result=json.loads(path.read_text(encoding='utf-8'))
        if result is None: result={'release':version,'error':log.read_text(encoding='utf-8',errors='replace')[-2000:]}
        result['returncode']=process.returncode
        result['log']=str(log)
        return result
    results=[]
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.workers) as pool:
        futures={pool.submit(run,version):version for version in versions}
        for future in concurrent.futures.as_completed(futures):
            result=future.result();results.append(result)
            (out/'results.json').write_text(json.dumps(results,indent=2)+'\n',encoding='utf-8')
            print(json.dumps({'release':result['release'],'passed':result['returncode']==0,
                              'checks':result.get('checks',{}),'error':result.get('error'),'run':result.get('run')}),flush=True)
    if any(result['returncode'] for result in results): raise SystemExit(1)


if __name__=='__main__':
    main()
