#!/usr/bin/env python3
"""Live signed-chat validation on an isolated, online-mode vanilla server.

The existing launcher handles account credentials through its normal keychain
and one-use session-file flow. This driver reads only public account metadata.
All runs use fresh homes. Nothing is installed into real launcher instances.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import time
import uuid

ROOT=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('vanilla',ROOT/'tools/vanilla-gameplay.py')
vanilla=importlib.util.module_from_spec(spec);spec.loader.exec_module(vanilla)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('release')
    parser.add_argument('--resources',type=Path,required=True)
    parser.add_argument('--launcher',type=Path,default=Path('D:/horizon/horizon-launcher.exe'))
    parser.add_argument('--accounts',type=Path,default=Path(os.environ['APPDATA'])/'horizon-launcher/accounts.json')
    parser.add_argument('--messages',type=int,default=25)
    args=parser.parse_args()
    assert 1<=args.messages<=100
    account_data=json.loads(args.accounts.read_text(encoding='utf-8'))
    assert set(account_data)<= {'accounts','selected','offline','offline_selected','looks'}
    account=next(a for a in account_data['accounts'] if a['uuid']==account_data['selected'])
    player=account['name'];profile=account['uuid']
    receipt=json.loads((ROOT/'target/native-game-build.json').read_text(encoding='utf-8'))
    assert receipt['kind']=='production'
    for item in receipt['binaries']:
        assert hashlib.sha256(Path(item['path']).read_bytes()).hexdigest()==item['sha256']
    jar=vanilla.download(args.release)
    run=ROOT/'target'/('online-chat-'+args.release+'-'+uuid.uuid4().hex[:8]);run.mkdir()
    env=vanilla.isolated_env(run);game=run/'game';server_dir=run/'server';server_dir.mkdir()
    launcher_home=Path(env['HORIZON_LAUNCHER_HOME'])
    # accounts.json contains only metadata; secrets remain in the OS keychain.
    (launcher_home/'accounts.json').write_text(json.dumps(account_data),encoding='utf-8')
    tag='native-vfp-chat-test';version=launcher_home/'versions'/tag;version.mkdir(parents=True)
    for item in receipt['binaries']:shutil.copy2(item['path'],version/Path(item['path']).name)
    (version/'.installed').write_text('',encoding='utf-8')
    prepared=version/'.local/horizon-client/26.3'
    # Copy only genuine readiness/completion metadata, never link player data.
    # The game receives --resources pointing to the complete prepared install.
    names=['horizon-resources.json','registries.json','commands.json',
           'assets/minecraft/font/unifont.zip','assets/pack.mcmeta','assets/minecraft/sounds.json',
           'assets/minecraft/resourcepacks/programmer_art.zip','assets/minecraft/resourcepacks/high_contrast.zip',
           'assets/minecraft/textures/gui/title/background/panorama_0.png']
    for name in names:
        source=args.resources/name;destination=prepared/name
        assert source.is_file(),f'Missing prepared completion file: {name}'
        destination.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(source,destination)
    data=game/'horizon-mods/.horizon-mod-data/viahorizon';data.mkdir(parents=True)
    (data/'config.json').write_text(json.dumps({'schema':1,'target':args.release,'login_preview':False,'servers':{}}),encoding='utf-8')
    (game/'options.txt').write_text('renderDistance:2\nsimulationDistance:2\nmaxFps:60\n',encoding='utf-8')
    port=vanilla.free_port()
    properties={'server-ip':'127.0.0.1','server-port':port,'online-mode':'true','enforce-secure-profile':'true',
                'enable-rcon':'false','enable-query':'false','view-distance':2,'simulation-distance':2,
                'level-type':'minecraft:flat','gamemode':'creative','max-players':2,
                'network-compression-threshold':256,'pause-when-empty-seconds':-1,'generate-structures':'false'}
    (server_dir/'server.properties').write_text(''.join(f'{k}={v}\n' for k,v in properties.items()),encoding='utf-8')
    (server_dir/'eula.txt').write_text('eula=true\n',encoding='utf-8')
    (server_dir/'ops.json').write_text(json.dumps([{'uuid':str(uuid.UUID(profile)),'name':player,'level':4,'bypassesPlayerLimit':False}]),encoding='utf-8')
    report={'release':args.release,'run':str(run),'online_mode':True,'enforce_secure_profile':True,
            'production_bundle_sha256':receipt['bundle_sha256'],'checks':{},'error':None}
    print(json.dumps({'run':str(run),'release':args.release}),flush=True)
    processes=[];handles=[]
    def log(name):
        handle=(run/name).open('wb');handles.append(handle);return handle
    def ctl(*words):
        result=subprocess.run([str(version/'horizon-ctl.exe'),'--socket',str(run/'ctl.sock'),'--timeout','10',*words],
                              cwd=game,env=env,capture_output=True,text=True,encoding='utf-8',timeout=15,creationflags=vanilla.HIDDEN)
        if result.returncode:raise RuntimeError('Control command failed: '+result.stderr)
        return json.loads(result.stdout)
    def wait(description,query,predicate,seconds=90):
        end=time.monotonic()+seconds;last=None
        while time.monotonic()<end:
            last=query()
            if predicate(last):return last
            time.sleep(.2)
        raise AssertionError(description+': '+str(last))
    def loaded():
        if client.poll() is not None:
            raise RuntimeError('Launcher exited before world entry; see local launcher.log')
        if not (run/'ctl.sock').exists():return False
        status=ctl('status')
        return status.get('screen')=='playing' and status.get('server') and status['server']['loaded'] and status['chunks']>=9
    try:
        server=subprocess.Popen([shutil.which('java'),'-Xms512M','-Xmx2G','-jar',str(jar),'nogui'],cwd=server_dir,env=env,
                                stdin=subprocess.PIPE,stdout=log('server.log'),stderr=subprocess.STDOUT,creationflags=vanilla.HIDDEN)
        processes.append(server)
        print('Waiting for isolated online-mode vanilla server',flush=True)
        wait('Vanilla server startup',lambda:(run/'server.log').read_text(encoding='utf-8',errors='replace'),lambda text:'Done (' in text,240)
        command=[str(args.launcher),'play','--version',tag]
        for word in ['--resources',str(args.resources),'--offscreen','--control-socket',str(run/'ctl.sock'),
                     '--render-distance','2','--server',f'127.0.0.1:{port}']:
            command.append('--arg='+word)
        client=subprocess.Popen(command,cwd=game,env=env,stdout=log('launcher.log'),stderr=subprocess.STDOUT,creationflags=vanilla.HIDDEN)
        processes.append(client)
        print('Waiting for authenticated Rust-client world entry',flush=True)
        wait('Authenticated world entry',loaded,bool,240);report['checks']['authenticated_world_entry']=True
        for index in range(args.messages):
            marker='vfp_signed_'+str(index)+'_'+uuid.uuid4().hex[:6]
            ctl('chat',marker)
            wait('Signed chat echo',lambda:ctl('messages'),lambda value:marker in json.dumps(value),20)
            if index%10==0:print(f'Verified signed chat message {index+1}',flush=True)
        report['checks']['signed_chat_messages']=args.messages
        ctl('disconnect');wait('Disconnected',lambda:ctl('state'),lambda state:state['screen']=='title')
        ctl('connect',f'127.0.0.1:{port}');wait('Authenticated reconnect',loaded,bool,120)
        marker='vfp_signed_reconnect_'+uuid.uuid4().hex[:6];ctl('chat',marker)
        wait('Signed reconnect echo',lambda:ctl('messages'),lambda value:marker in json.dumps(value),20)
        report['checks']['signed_chat_after_reconnect']=True
        status=ctl('status');assert status['server']['decode_errors']==0,status['server']
        report['checks']['zero_decode_errors']=True
        (run/'final-status.json').write_text(json.dumps(status,indent=2),encoding='utf-8')
        ctl('quit');client.wait(timeout=30)
        assert client.returncode==0,client.returncode
    except Exception as error:
        report['error']=str(error)
    finally:
        if 'client' in locals() and client.poll() is None:
            try:ctl('quit');client.wait(timeout=20)
            except Exception:
                # Kill only this isolated launcher's process tree on Windows.
                subprocess.run(['taskkill','/PID',str(client.pid),'/T','/F'],capture_output=True,creationflags=vanilla.HIDDEN)
                client.wait(timeout=15)
        if 'server' in locals() and server.poll() is None:
            try:server.stdin.write(b'stop\n');server.stdin.flush();server.wait(timeout=30)
            except Exception:server.kill();server.wait()
        for handle in handles:handle.close()
        report['processes_stopped']=all(p.poll() is not None for p in processes)
        (run/'result.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
        print(json.dumps(report),flush=True)
    if report['error']:raise SystemExit(1)


if __name__=='__main__':main()
