#!/usr/bin/env python3
"""Isolated real-vanilla-server / Rust-client compatibility harness.

Java is used only for the vanilla test server. Nothing is installed into the
launcher's instances. All worlds, resources downloaded here, logs and reports
stay in target/. The caller must have permission to run the vanilla server.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import socket
import subprocess
import time
import threading
import urllib.request
import uuid
import zlib

ROOT = Path(__file__).resolve().parents[1]
CACHE = ROOT/'target'/'vanilla-servers'
HIDDEN = subprocess.CREATE_NO_WINDOW if os.name == 'nt' else 0


def fetch(url, path, digest, size=None):
    if path.exists() and hashlib.sha1(path.read_bytes()).hexdigest() == digest:
        return path.read_bytes()
    if not url.startswith(('https://piston-meta.mojang.com/', 'https://piston-data.mojang.com/',
                           f'https://launcher.mojang.com/v1/objects/{digest}/')):
        raise ValueError('Download must use Mojang content-addressed metadata')
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix+'.download-'+uuid.uuid4().hex)
    try:
        with urllib.request.urlopen(url, timeout=45) as response, temporary.open('wb') as out:
            count = 0
            while data := response.read(1024*1024):
                count += len(data)
                if count > (size if size is not None else 8*1024*1024):
                    raise ValueError('Download exceeds advertised size')
                out.write(data)
        data = temporary.read_bytes()
        if size is not None and len(data) != size:
            raise ValueError('Truncated server download')
        if hashlib.sha1(data).hexdigest() != digest:
            raise ValueError('Mojang download checksum differs')
        temporary.replace(path)
        return data
    finally:
        temporary.unlink(missing_ok=True)


def download(release):
    catalog_path = ROOT/'target/gameplay-catalog.json'
    if not catalog_path.exists():
        catalog_path = ROOT/'tools/data/minecraft-versions.json'
    catalog = json.loads(catalog_path.read_text(encoding='utf-8'))
    entry = next(item for item in catalog if item['id'] == release)
    raw = fetch(entry['url'], CACHE/release/'version.json', entry['sha1'])
    metadata = json.loads(raw)
    server = metadata['downloads']['server']
    path = CACHE/release/'server.jar'
    fetch(server['url'], path, server['sha1'], server['size'])
    print(json.dumps({'release':release,'jar':str(path),'sha1':server['sha1'],
                      'java_major':metadata.get('javaVersion',{}).get('majorVersion')}),flush=True)
    return path


def free_port():
    with socket.socket() as sock:
        sock.bind(('127.0.0.1',0))
        return sock.getsockname()[1]


def release_protocol(release):
    def version(value):
        parts = tuple(map(int, value.split('.')))
        return parts + (0,) * (3 - len(parts))

    catalog = (ROOT/'mixins/viahorizon/core.rs').read_text(encoding='utf-8')
    releases = re.findall(r'first: "([\d.]+)",\s+last: "([\d.]+)",\s+'
                          r'protocol: (\d+),\s+legacy: false,', catalog)
    for first, last, protocol in releases:
        if version(first) <= version(release) <= version(last):
            return int(protocol)
    raise ValueError(f'Unknown Netty Release: {release}')


class TranscriptProxy:
    """Forward unmodified offline traffic and save bounded decoded transcripts."""
    def __init__(self, upstream, run, protocol):
        self.upstream, self.run, self.protocol = upstream, run, protocol
        self.phase, self.compressed = 'login', False
        self.sockets, self.threads = [], []
        self.lock = threading.Lock()
        self.recorded = 0
        self.hold_next_block_use = False
        self.block_use_held = threading.Event()
        self.release_block_use = threading.Event()
        self.log = (run/'wire.jsonl').open('w', encoding='utf-8')
        self.listener = socket.socket()
        self.listener.bind(('127.0.0.1',0))
        self.listener.listen(1)
        self.port = self.listener.getsockname()[1]
        thread=threading.Thread(target=self.accept,daemon=True)
        self.threads.append(thread);thread.start()

    @staticmethod
    def varint(data, offset=0):
        value=0
        for index in range(5):
            byte=data[offset+index]
            value|=(byte&127)<<(index*7)
            if byte<128: return value,offset+index+1
        raise ValueError('Oversized wire VarInt')

    @staticmethod
    def exact(sock, count):
        chunks=[]
        while count:
            part=sock.recv(count)
            if not part: raise EOFError()
            chunks.append(part);count-=len(part)
        return b''.join(chunks)

    def accept(self):
        try:
            client,_=self.listener.accept()
            server=socket.create_connection(('127.0.0.1',self.upstream),timeout=10)
            server.settimeout(None)
            self.sockets.extend([client,server])
            for source,destination,direction in [(client,server,'serverbound'),(server,client,'clientbound')]:
                thread=threading.Thread(target=self.pipe,args=(source,destination,direction),daemon=True)
                self.threads.append(thread);thread.start()
        except OSError: pass

    def pipe(self, source, destination, direction):
        try:
            while True:
                prefix=bytearray()
                for _ in range(5):
                    byte=self.exact(source,1);prefix.extend(byte)
                    if byte[0]<128: break
                size,_=self.varint(prefix)
                if size>2_097_152: raise ValueError('Oversized frame')
                framed=self.exact(source,size)
                data=framed
                if self.compressed:
                    length,offset=self.varint(data)
                    data=zlib.decompress(data[offset:]) if length else data[offset:]
                    if length and len(data)!=length: raise ValueError('Bad compression length')
                identifier,offset=self.varint(data)
                payload=data[offset:]
                with self.lock:
                    phase=self.phase
                    if self.recorded+len(payload)<64*1024*1024:
                        self.log.write(json.dumps({'direction':direction,'phase':phase,'id':identifier,
                                                   'payload':payload.hex()})+'\n');self.log.flush()
                        self.recorded+=len(payload)
                    if direction=='clientbound' and phase=='login':
                        if identifier==3: self.compressed=True
                        if identifier==2 and self.protocol<764: self.phase='play'
                    if direction=='serverbound':
                        if phase=='login' and identifier==3 and self.protocol>=764:
                            self.phase='configuration'
                        elif phase=='configuration' and identifier==(2 if self.protocol<766 else 3):
                            self.phase='play'
                if (self.hold_next_block_use and direction=='serverbound'
                        and phase=='play' and self.protocol in (4,5,47) and identifier==8):
                    self.hold_next_block_use=False
                    self.block_use_held.set()
                    if not self.release_block_use.wait(timeout=15):
                        raise TimeoutError('Held block use was not released by the test')
                destination.sendall(prefix+framed)
        except (EOFError,OSError): pass
        except Exception as error:
            with self.lock:
                self.log.write(json.dumps({'proxy_error':str(error)})+'\n');self.log.flush()
        finally:
            try: destination.shutdown(socket.SHUT_WR)
            except OSError: pass

    def close(self):
        self.release_block_use.set()
        self.listener.close()
        for sock in self.sockets:
            try: sock.shutdown(socket.SHUT_RDWR)
            except OSError: pass
            sock.close()
        for thread in self.threads: thread.join(timeout=2)
        self.log.close()


def isolated_env(run):
    env = os.environ.copy()
    for key, suffix in [('HORIZON_HOME','game'), ('HORIZON_LAUNCHER_HOME','launcher'),
                        ('XDG_DATA_HOME','xdg-data'),('XDG_CONFIG_HOME','xdg-config'),
                        ('HORIZON_LAUNCHER_TRASH','trash'),('HORIZON_LAUNCHER_CLIPBOARD','clipboard'),
                        ('HORIZON_LAUNCHER_DESKTOP','desktop')]:
        path = run/suffix
        path.mkdir(parents=True,exist_ok=True)
        env[key] = str(path)
    env.update(HORIZON_LAUNCHER_NO_SELF_UPDATE='1', HORIZON_NO_AUDIO='1',
               HORIZON_NET_DEBUG='1', RUST_BACKTRACE='1')
    return env


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('release')
    parser.add_argument('--download-only',action='store_true')
    parser.add_argument('--join-only',action='store_true',help='Only check world entry and chunk decoding')
    parser.add_argument('--terrain',choices=['flat','normal'],default='flat')
    parser.add_argument('--auto',action='store_true',help='Test status-based target selection without the transcript proxy')
    parser.add_argument('--interaction-regression',action='store_true',
                        help='Check early-release air use and placement before the server receives the click')
    parser.add_argument('--legacy-regressions',action='store_true',help='Check 1.7 beds, crafting output, sword use, explosion light and unsupported items')
    parser.add_argument('--streaming-profile',action='store_true',
                        help='Measure idle and repeated new-chunk frame times in an isolated world')
    parser.add_argument('--profile-origin',nargs=3,type=float,metavar=('X','Y','Z'),
                        help='Use a fixed start position for matched streaming comparisons')
    parser.add_argument('--transfer-from',help='First join this vanilla release, then use its transfer command to reach the target')
    parser.add_argument('--resources',type=Path)
    parser.add_argument('--java',default=shutil.which('java'))
    parser.add_argument('--client',type=Path,default=ROOT/'target/gameplay-probe/horizon-client.exe')
    parser.add_argument('--ctl',type=Path,default=ROOT/'target/gameplay-probe/horizon-ctl.exe')
    parser.add_argument('--timeout',type=int,default=180)
    args=parser.parse_args()
    args.client=args.client.resolve()
    args.ctl=args.ctl.resolve()
    if args.resources:
        args.resources=args.resources.resolve()
    if args.transfer_from: args.auto=True
    jar=download(args.release)
    origin_jar=download(args.transfer_from) if args.transfer_from else None
    if args.download_only:
        return
    if not args.java or not args.resources:
        parser.error('Need Java and prepared 26.3 --resources')
    run=ROOT/'target'/('vplay-'+args.release+'-'+uuid.uuid4().hex[:8])
    server_dir=run/'server'
    server_dir.mkdir(parents=True)
    env=isolated_env(run)
    game=run/'game'
    data=game/'horizon-mods/.horizon-mod-data/viahorizon'
    data.mkdir(parents=True)
    (data/'config.json').write_text(json.dumps({'schema':1,'target':'auto' if args.auto else args.release,
                                               'login_preview':False,'servers':{}}),encoding='utf-8')
    distance=6 if args.streaming_profile else 2
    options=f'renderDistance:{distance}\nsimulationDistance:2\nmaxFps:{120 if args.streaming_profile else 60}\n'
    if args.streaming_profile: options+='enableVsync:false\n'
    (game/'options.txt').write_text(options,encoding='utf-8')
    (game/'horizon-controller.txt').write_text('enabled:false\n',encoding='utf-8')
    port=free_port()
    protocol=release_protocol(args.release)
    if args.interaction_regression and (args.auto or args.join_only or protocol not in (4,5,47)):
        parser.error('--interaction-regression needs manual targeting, full gameplay and 1.7/1.8')
    properties={'server-ip':'127.0.0.1','server-port':str(port),'online-mode':'false',
                'enforce-secure-profile':'false','enable-rcon':'false','enable-query':'false',
                'view-distance':str(distance) if args.streaming_profile else '4' if protocol<757 else '2',
                'simulation-distance':'2','spawn-protection':'0',
                'level-type':('minecraft:' if protocol>=759 else '')+args.terrain,
                'level-seed':'1','gamemode':'1' if protocol<393 else 'creative',
                'max-players':'2','network-compression-threshold':'256',
                'generate-structures':'false','sync-chunk-writes':'false',
                'pause-when-empty-seconds':'-1','motd':'Isolated ViaHorizon compatibility test'}
    (server_dir/'server.properties').write_text(''.join(f'{k}={v}\n' for k,v in properties.items()),encoding='utf-8')
    (server_dir/'eula.txt').write_text('eula=true\n',encoding='utf-8')
    offline=uuid.UUID(bytes=hashlib.md5(b'OfflinePlayer:VfpSmoke').digest(),version=3)
    (server_dir/'ops.json').write_text(json.dumps([{'uuid':str(offline),'name':'VfpSmoke',
                                                  'level':4,'bypassesPlayerLimit':False}]),encoding='utf-8')
    if protocol == 4:
        (server_dir/'ops.txt').write_text('VfpSmoke\n',encoding='utf-8')
    print(json.dumps({'run':str(run),'port':port}),flush=True)
    processes=[]
    proxies=[]
    report={'release':args.release,'terrain':args.terrain,'auto':args.auto,'run':str(run),'world_entry':False,'checks':{},'error':None}
    log_handles=[]
    def log(name):
        handle=(run/name).open('wb');log_handles.append(handle);return handle
    def console(command):
        server.stdin.write((command+'\n').encode('utf-8'));server.stdin.flush()
    def ctl(*command):
        result=subprocess.run([str(args.ctl),'--socket',str(run/'ctl.sock'),'--timeout','10',*command],
                              cwd=game,env=env,capture_output=True,text=True,encoding='utf-8',
                              timeout=15,creationflags=HIDDEN)
        if result.returncode:
            raise RuntimeError('ctl '+repr(command)+': '+(result.stderr or result.stdout))
        return json.loads(result.stdout)
    def wait_value(description, query, predicate, seconds=20):
        deadline=time.monotonic()+seconds
        value=None
        while time.monotonic()<deadline:
            value=query()
            if predicate(value): return value
            time.sleep(.2)
        raise AssertionError(description+': '+json.dumps(value))
    def authoritative(command, marker):
        before=(run/'server.log').stat().st_size
        console(command if protocol<393 else command+' run say '+marker)
        def output():
            with (run/'server.log').open('rb') as handle:
                handle.seek(before)
                return handle.read().decode('utf-8',errors='replace')
        wait_value('Server authoritative check '+marker,output,lambda text: marker in text)

    def check_block(x, y, z, name, marker):
        if protocol<393:
            authoritative(f'testforblock {x} {y} {z} minecraft:{name}',
                          'Successfully found the block')
        else:
            authoritative(f'execute if block {x} {y} {z} minecraft:{name}',marker)

    def gamemode(name):
        mode = {'survival':'0','creative':'1'}[name] if protocol<393 else name
        console(f'gamemode {mode} VfpSmoke')
        wait_value(name.title()+' mode',lambda:ctl('state'),lambda state:state['gamemode']==name)

    def hotbar_has(item, slot):
        return any(entry.get('slot')==slot and entry.get('item')==item
                   for entry in ctl('inventory')['hotbar'])

    def supply(item, count):
        if protocol<47:
            # Early /give throws the stack forward. Zero its velocity so this
            # fixture depends on item pickup, rather than where it lands.
            x,y,z=ctl('state')['player']['position']
            ident={'stone':1,'oak_planks':5,'diamond_sword':276,'apple':260}[item]
            console(f'summon Item {x} {y} {z} {{Item:{{id:{ident}s,Count:{count}b,Damage:0s}},'
                    'Motion:[0.0d,0.0d,0.0d],PickupDelay:0s}')
        else:
            name='planks' if item=='oak_planks' and protocol<393 else item
            console(f'give VfpSmoke minecraft:{name} {count}')
    try:
        server=subprocess.Popen([args.java,'-Xms512M','-Xmx2G','-jar',str(jar),'nogui'],cwd=server_dir,
                                env=env,stdin=subprocess.PIPE,stdout=log('server.log'),stderr=subprocess.STDOUT,
                                creationflags=HIDDEN)
        processes.append(server)
        deadline=time.monotonic()+args.timeout
        last_progress=0
        while True:
            text=(run/'server.log').read_text(encoding='utf-8',errors='replace')
            if 'Done (' in text and 'For help' in text:
                break
            if server.poll() is not None or 'Encountered an unexpected exception' in text:
                raise RuntimeError('Vanilla server exited before ready: '+text[-2500:])
            if time.monotonic()>deadline:
                raise TimeoutError('Server startup timed out')
            if time.monotonic()-last_progress>15:
                print('Waiting for vanilla server '+args.release,flush=True);last_progress=time.monotonic()
            time.sleep(.5)
        deadline=time.monotonic()+args.timeout
        def probe_ready():
            if not args.client.is_file() or not args.ctl.is_file(): return False
            if args.client.parent!=ROOT/'target/gameplay-probe': return True
            path=args.client.parent/'build.json'
            if not path.is_file(): return False
            try: provenance=json.loads(path.read_text(encoding='utf-8'))
            except json.JSONDecodeError: return False
            digest=hashlib.sha256()
            for path in sorted((ROOT/'mixins').rglob('*')):
                if path.is_file() and path.suffix in ('.rs','.toml'):
                    digest.update(path.relative_to(ROOT).as_posix().encode());digest.update(b'\0');digest.update(path.read_bytes())
            return provenance['base_bundle_sha256']==digest.hexdigest()
        while not probe_ready():
            if time.monotonic()>deadline:
                raise TimeoutError('Gameplay probe executables are not built')
            if time.monotonic()-last_progress>15:
                print('Waiting for gameplay probe executable',flush=True);last_progress=time.monotonic()
            time.sleep(.5)
        if args.client.parent==ROOT/'target/gameplay-probe':
            provenance=json.loads((args.client.parent/'build.json').read_text(encoding='utf-8'))
            digest=hashlib.sha256()
            for path in sorted((ROOT/'mixins').rglob('*')):
                if path.is_file() and path.suffix in ('.rs','.toml'):
                    digest.update(path.relative_to(ROOT).as_posix().encode());digest.update(b'\0');digest.update(path.read_bytes())
            if provenance['base_bundle_sha256']!=digest.hexdigest():
                raise RuntimeError('Gameplay probe is stale; rebuild before testing')
            for executable in (args.client,args.ctl):
                record=next(item for item in provenance['executables'] if item['file']==executable.name)
                with executable.open('rb') as binary:
                    assert hashlib.file_digest(binary,'sha256').hexdigest()==record['sha256'],executable
            report['probe_base_bundle_sha256']=digest.hexdigest()
        else:
            receipt_path=args.client.parent/'build.json'
            if not receipt_path.is_file(): receipt_path=ROOT/'target/native-game-build.json'
            receipt=json.loads(receipt_path.read_text(encoding='utf-8'))
            assert receipt['kind']=='production'
            for executable in (args.client,args.ctl):
                # Installed copies have a different path; require the same
                # binary name and content as the production build receipt.
                record=next((item for item in receipt['binaries']
                             if Path(item['path']).name==executable.name), None)
                if record is None:
                    raise RuntimeError('Executable absent from production receipt: '+str(executable))
                assert hashlib.sha256(executable.read_bytes()).hexdigest()==record['sha256'],'Executable differs from build receipt'
            report['production_bundle_sha256']=receipt['bundle_sha256']
        if origin_jar:
            origin_dir=run/'transfer-origin';origin_dir.mkdir()
            origin_port=free_port()
            origin_properties={**properties,'server-port':str(origin_port)}
            (origin_dir/'server.properties').write_text(''.join(f'{k}={v}\n' for k,v in origin_properties.items()),encoding='utf-8')
            for name in ('eula.txt','ops.json'): shutil.copyfile(server_dir/name,origin_dir/name)
            origin=subprocess.Popen([args.java,'-Xms512M','-Xmx2G','-jar',str(origin_jar),'nogui'],cwd=origin_dir,
                                    env=env,stdin=subprocess.PIPE,stdout=log('transfer-server.log'),stderr=subprocess.STDOUT,
                                    creationflags=HIDDEN)
            processes.append(origin)
            deadline=time.monotonic()+args.timeout
            while 'Done (' not in (run/'transfer-server.log').read_text(encoding='utf-8',errors='replace'):
                if origin.poll() is not None: raise RuntimeError('Transfer origin server exited')
                if time.monotonic()>deadline: raise TimeoutError('Transfer origin startup timed out')
                time.sleep(.5)
        if args.auto:
            connect_port=origin_port if origin_jar else port
        else:
            proxy=TranscriptProxy(port,run,protocol);proxies.append(proxy)
            connect_port=proxy.port
        client=subprocess.Popen([str(args.client),'--resources',str(args.resources),'--offscreen',
                                 '--control-socket',str(run/'ctl.sock'),'--smoke-report',str(run/'game-report.json'),
                                 '--name','VfpSmoke','--width','1280','--height','720','--render-distance',str(distance),
                                 '--server',f'127.0.0.1:{connect_port}'],cwd=game,env=env,
                                stdout=log('client-stdout.log'),stderr=log('client-stderr.log'),creationflags=HIDDEN)
        processes.append(client)
        if origin_jar:
            deadline=time.monotonic()+args.timeout
            while not (run/'ctl.sock').exists():
                if client.poll() is not None: raise RuntimeError('Client exited before transfer origin was ready')
                if time.monotonic()>deadline: raise TimeoutError('Transfer client startup timed out')
                time.sleep(.5)
            def at_server(value,server_port):
                return (value.get('server') and value['server']['address'].endswith(':'+str(server_port))
                        and value['screen']=='playing' and value['server']['loaded'] and value['chunks']>=9)
            wait_value('Transfer origin world entry',lambda:ctl('status'),lambda value:at_server(value,origin_port),seconds=args.timeout)
            origin.stdin.write(f'transfer 127.0.0.1 {port} VfpSmoke\n'.encode());origin.stdin.flush()
            transferred=wait_value('Cross-version transfer world entry',lambda:ctl('status'),
                                   lambda value:at_server(value,port),seconds=args.timeout)
            assert transferred['server']['decode_errors']==0,transferred['server']
            (run/'transfer-status.json').write_text(json.dumps(transferred,indent=2),encoding='utf-8')
            report['checks']['cross_version_transfer']=True
        deadline=time.monotonic()+args.timeout
        while time.monotonic()<deadline:
            if client.poll() is not None:
                raise RuntimeError('Rust client exited before world entry')
            if (run/'ctl.sock').exists():
                state=ctl('state')
                (run/'latest-state.json').write_text(json.dumps(state,indent=2),encoding='utf-8')
                if state.get('player') and state.get('screen')=='playing':
                    report['world_entry']=True
                    break
                if state.get('screen')=='disconnected' or (state.get('menu') or {}).get('disconnected'):
                    raise RuntimeError('Client disconnected: '+json.dumps(ctl('screen')))
            if time.monotonic()-last_progress>15:
                print('Waiting for Rust client world entry '+args.release,flush=True);last_progress=time.monotonic()
            time.sleep(.5)
        if not report['world_entry']:
            raise TimeoutError('Client did not enter the world')
        status=wait_value('Loaded and meshed chunks',lambda:ctl('status'),
                          lambda s:s['chunks']>=9 and s['meshed_sections']>0 and s['server']['loaded'])
        (run/'world-status.json').write_text(json.dumps(status,indent=2),encoding='utf-8')
        report['checks']['player_spawned']=True
        assert status['server']['decode_errors']==0,status['server']
        report['checks']['chunks_and_native_decoders']=True
        if args.streaming_profile:
            print('Profiling idle and new chunks: '+args.release,flush=True)
            if args.profile_origin:
                px,py,pz=args.profile_origin
                console(f'tp VfpSmoke {px} {py} {pz}')
                wait_value('Profile origin',lambda:ctl('state'),
                           lambda state:abs(state['player']['position'][0]-px)<1
                           and abs(state['player']['position'][2]-pz)<1)
            # Reset after initial world entry, then compare stationary rendering
            # with deterministic travel into previously unseen terrain.
            time.sleep(4)
            ctl('stats','reset');time.sleep(5)
            idle=ctl('stats')
            ctl('stats','reset')
            position=ctl('state')['player']['position']
            samples=[]
            for index in range(1,9):
                console(f'tp VfpSmoke {int(position[0])+index*64} 90 {int(position[2])}')
                time.sleep(2)
                samples.append(ctl('status'))
            streaming=ctl('stats')
            profile={'idle':idle,'streaming':streaming,'samples':samples,
                     'distance':distance,'steps':8,'step_blocks':64}
            (run/'streaming-profile.json').write_text(json.dumps(profile,indent=2),encoding='utf-8')
            assert all(s['server'] and s['server']['decode_errors']==0 for s in samples)
            report['checks']['chunk_streaming_profile']=True
        if not args.join_only:
            print('Checking movement/chat/inventory/blocks: '+args.release,flush=True)
            start=ctl('state')['player']['position']
            ctl('move','forward','0.6')
            moved=ctl('state')['player']['position']
            assert sum((a-b)**2 for a,b in zip(start,moved))>.1,(start,moved)
            time.sleep(.3)
            settled=ctl('state')['player']['position']
            assert sum((a-b)**2 for a,b in zip(start,settled))>.1,(start,settled)
            report['checks']['movement']=True
            marker='vfp_chat_'+uuid.uuid4().hex[:8]
            ctl('chat',marker)
            wait_value('Chat echo',lambda:ctl('messages'),lambda messages:marker in json.dumps(messages))
            report['checks']['chat']=True
            supply('stone',16)
            inventory=wait_value('Stone inventory',lambda:ctl('inventory'),
                                 lambda inv:inv.get('counts',{}).get('minecraft:stone',0)>=16)
            (run/'inventory-before.json').write_text(json.dumps(inventory,indent=2),encoding='utf-8')
            ctl('select','minecraft:stone')
            if args.interaction_regression:
                for item in ('diamond_sword','apple'):
                    supply(item,1)
                    wait_value(item+' inventory',lambda:ctl('inventory'),
                               lambda inv:inv.get('counts',{}).get('minecraft:'+item,0)>=1)
                ctl('look','0','-89')
                for item in ('minecraft:stone','minecraft:diamond_sword','minecraft:apple','9'):
                    ctl('select',item)
                    for _ in range(3):
                        ctl('use-item','--hold','0.25')
                        assert ctl('state')['screen']=='playing','Air right click disconnected'
                report['checks']['repeated_air_right_clicks']=True
                ctl('select','minecraft:stone')
            position=ctl('state')['player']['block_pos']
            x,y,z=position[0]+2,position[1],position[2]+1
            if args.interaction_regression:
                proxy.hold_next_block_use=True
            try:
                ctl('place',str(x),str(y),str(z),'--item','minecraft:stone')
                if args.interaction_regression:
                    assert proxy.block_use_held.wait(timeout=5),'Placement packet was not intercepted'
                    samples=[]
                    for _ in range(5):
                        block=ctl('block',str(x),str(y),str(z))
                        samples.append(block)
                        assert block.get('block')=='minecraft:stone',block
                        time.sleep(.1)
                    (run/'placement-before-server.json').write_text(json.dumps(samples,indent=2),encoding='utf-8')
                    report['checks']['placement_kept_before_server_reply']=True
            finally:
                if args.interaction_regression:
                    proxy.release_block_use.set()
            check_block(x,y,z,'stone','vfp_block_placed')
            if args.interaction_regression:
                console(f'setblock {x} {y} {z} minecraft:air')
                wait_value('Server correction replaces local placement',
                           lambda:ctl('block',str(x),str(y),str(z)),
                           lambda block:block.get('block')=='minecraft:air')
                report['checks']['authoritative_placement_correction']=True
                ctl('place',str(x),str(y),str(z),'--item','minecraft:stone')
                check_block(x,y,z,'stone','vfp_block_replaced')
            ctl('break',str(x),str(y),str(z),'--timeout','10')
            check_block(x,y,z,'air','vfp_block_broken')
            report['checks']['block_placement_and_breaking']=True
            gamemode('survival')
            ctl('open-inventory')
            inventory=ctl('inventory')
            stone=next(item for item in inventory['hotbar'] if item.get('item')=='minecraft:stone')
            empty=next(item for item in inventory['hotbar'] if not item.get('item'))
            source_slot=stone['slot'];destination_slot=empty['slot']
            ctl('click-slot',str(source_slot));ctl('click-slot',str(destination_slot))
            ctl('close')
            if protocol>=393:
                authoritative('execute if entity @a[name=VfpSmoke,nbt={Inventory:[{Slot:'+str(destination_slot)+'b,id:"minecraft:stone"}]}]',
                              'vfp_inventory_moved')
                report['checks']['inventory_clicks']=True
            (run/'inventory-after.json').write_text(json.dumps(ctl('inventory'),indent=2),encoding='utf-8')
            supply('oak_planks',4)
            inventory=wait_value('Crafting ingredients',lambda:ctl('inventory'),
                                 lambda inv:inv.get('counts',{}).get('minecraft:oak_planks',0)>=4)
            planks=next(item for item in inventory['hotbar'] if item.get('item')=='minecraft:oak_planks')
            ctl('open-inventory');ctl('click-slot',str(planks['slot']))
            for slot in range(100,104): ctl('click-slot',str(slot),'right')
            wait_value('Crafting output',lambda:ctl('screen'),
                       lambda screen:any(item.get('slot')==200 and item.get('item')=='minecraft:crafting_table'
                                         for item in screen.get('slots',[])))
            ctl('click-slot','200');ctl('click-slot',str(planks['slot']));ctl('close')
            if protocol>=393:
                authoritative('execute if entity @a[name=VfpSmoke,nbt={Inventory:[{id:"minecraft:crafting_table"}]}]',
                              'vfp_crafting_result')
                report['checks']['crafting']=True
            if protocol<393:
                item = '264s' if protocol<47 else '"minecraft:diamond"'
                console(f'setblock {x} {y} {z} minecraft:chest 2 replace '
                        f'{{Items:[{{Slot:0b,id:{item},Count:3b,Damage:0s}}]}}')
            else:
                console(f'setblock {x} {y} {z} minecraft:chest')
                if protocol<755:
                    console(f'replaceitem block {x} {y} {z} container.0 minecraft:diamond 3')
                else:
                    console(f'item replace block {x} {y} {z} container.0 with minecraft:diamond 3')
            wait_value('Chest block update',lambda:ctl('block',str(x),str(y),str(z)),
                       lambda block:'minecraft:chest' in json.dumps(block))
            ctl('interact',str(x),str(y),str(z))
            wait_value('Chest contents',lambda:ctl('screen'),
                       lambda screen:any(slot.get('slot')==300 and slot.get('item')=='minecraft:diamond'
                                         for slot in screen.get('slots',[])))
            ctl('click-slot','300','shift');ctl('close')
            if protocol>=393:
                authoritative('execute if entity @a[name=VfpSmoke,nbt={Inventory:[{id:"minecraft:diamond"}]}]',
                              'vfp_chest_transfer')
                report['checks']['container_transfer']=True
            if args.legacy_regressions:
                assert protocol in (4,5), '--legacy-regressions requires 1.7'
                ctl('render-distance','8')
                boundary=wait_value('Legacy boundary meshes finish',lambda:ctl('status'),
                    lambda value:value['chunks']>=81 and value['pending_chunks']==0 and value['streaming']['meshing']==0
                                 and value.get('renderer',{}).get('resident_sections',0)>=value['chunks'],
                    seconds=60)
                (run/'chunk-boundary-status.json').write_text(json.dumps(boundary,indent=2),encoding='utf-8')
                report['checks']['legacy_boundary_meshes_finish']=True
                ctl('render-distance','2')
                # Exercise the real 3x3 menu after the ordinary 2x2 crafting check.
                supply('oak_planks',2)
                inv=wait_value('Table ingredients',lambda:ctl('inventory'),
                               lambda inv:inv.get('counts',{}).get('minecraft:oak_planks',0)>=2)
                planks=next(item for item in inv['hotbar'] if item.get('item')=='minecraft:oak_planks')
                console(f'setblock {x} {y} {z} minecraft:crafting_table')
                wait_value('Crafting table block',lambda:ctl('block',str(x),str(y),str(z)),
                           lambda b:b.get('block')=='minecraft:crafting_table')
                ctl('interact',str(x),str(y),str(z))
                wait_value('3x3 crafting menu',lambda:ctl('screen'),lambda screen:screen.get('craft_size')==3)
                ctl('click-slot',str(planks['slot']))
                ctl('click-slot','100','right');ctl('click-slot','103','right')
                output=wait_value('Table output visible',lambda:ctl('screen'),
                    lambda screen:any(item.get('slot')==200 and item.get('item')=='minecraft:stick'
                                      for item in screen.get('slots',[])))
                (run/'crafting-table-output.json').write_text(json.dumps(output,indent=2),encoding='utf-8')
                ctl('click-slot','200');ctl('click-slot',str(planks['slot']));ctl('close')
                report['checks']['crafting_table_output_visible']=True
                supply('diamond_sword',1)
                wait_value('Sword inventory',lambda:ctl('inventory'),
                           lambda inv:inv.get('counts',{}).get('minecraft:diamond_sword',0)>0)
                ctl('select','minecraft:diamond_sword');ctl('look','0','-89')
                ctl('mouse','right','down')
                try:
                    samples=[]
                    for _ in range(5):
                        time.sleep(.25);samples.append(ctl('state'))
                    assert all(sample.get('using') for sample in samples),samples
                    assert samples[-1]['using']['ticks']>samples[0]['using']['ticks'],samples
                    (run/'sword-use.json').write_text(json.dumps(samples,indent=2),encoding='utf-8')
                finally: ctl('mouse','right','up')
                wait_value('Sword release',lambda:ctl('state'),lambda state:state.get('using') is None)
                report['checks']['sword_use_remains_active_until_released']=True
                # Old beds stop downward velocity instead of the modern automatic bounce.
                pos=ctl('state')['player']['block_pos'];bx,by,bz=pos[0]+2,pos[1],pos[2]+2
                console(f'setblock {bx} {by} {bz} minecraft:bed 0')
                console(f'setblock {bx} {by} {bz+1} minecraft:bed 8')
                wait_value('Bed update',lambda:ctl('block',str(bx),str(by),str(bz)),lambda b:'bed' in b.get('block',''))
                base=by+9/16
                console(f'tp VfpSmoke {bx+.5} {base} {bz+.5}')
                wait_value('Standing on bed',lambda:ctl('state'),lambda state:abs(state['player']['position'][1]-base)<.1)
                ctl('key','space','down')
                try:
                    jumps=[]
                    for _ in range(12):
                        time.sleep(.08);jumps.append(ctl('state')['player']['position'][1])
                    assert max(jumps)>base+.6,jumps
                finally: ctl('key','space','up')
                wait_value('Landed on bed',lambda:ctl('state'),lambda state:state['player']['on_ground'] and abs(state['player']['position'][1]-base)<.1)
                (run/'bed-jump.json').write_text(json.dumps({'base':base,'samples':jumps},indent=2),encoding='utf-8')
                report['checks']['jump_and_land_on_legacy_bed']=True
                gamemode('creative')
                for item in ['turtle_helmet','elytra','netherite_sword','shield','trident']:
                    ctl('give',item,'1')
                    wait_value('Unsupported item removed: '+item,lambda:ctl('inventory'),
                               lambda inv:item not in json.dumps(inv))
                    ctl('use-item','--hold','0.1')
                    assert ctl('state')['screen']=='playing'
                report['checks']['unsupported_new_items_cannot_be_equipped_or_sent']=True
                ex,ey,ez=bx+4,by,bz
                console(f'setblock {ex} {ey} {ez} minecraft:dirt')
                wait_value('Explosion target',lambda:ctl('block',str(ex),str(ey),str(ez)),lambda b:b.get('block')=='minecraft:dirt')
                console(f'summon PrimedTnt {ex+.5} {ey+.5} {ez+.5} {{Fuse:5}}')
                wait_value('Explosion removed block',lambda:ctl('block',str(ex),str(ey),str(ez)),lambda b:b.get('block')=='minecraft:air')
                light=wait_value('Explosion retains skylight',lambda:ctl('light',str(ex),str(ey+1),str(ez),str(ex),str(ey+1),str(ez)),
                                 lambda value:value.get('sky') in ('e','f'))
                (run/'explosion-light.json').write_text(json.dumps(light,indent=2),encoding='utf-8')
                report['checks']['explosion_preserves_skylight']=True
                gamemode('survival')
            position=ctl('state')['player']['block_pos']
            hx,hy,hz=position[0]+1,position[1],position[2]+1
            mob = 'Zombie' if protocol<315 else 'minecraft:zombie' if protocol<393 else 'minecraft:husk'
            identifier = 'minecraft:zombie' if protocol<393 else 'minecraft:husk'
            console(f'summon {mob} {hx} {hy} {hz} {{NoAI:1b,Silent:1b,PersistenceRequired:1b,Tags:["vfp_test"]}}')
            mobs=wait_value('Mob spawn',lambda:ctl('entities','8',identifier),lambda value:bool(value.get('entities')))
            entity_id=mobs['entities'][0]['id']
            if protocol>=393:
                authoritative('execute if entity @e[tag=vfp_test,nbt={Health:20.0f}]','vfp_husk_ready')
            ctl('attack',str(entity_id))
            if protocol>=393:
                authoritative('execute unless entity @e[tag=vfp_test,nbt={Health:20.0f}]','vfp_husk_hit')
                console('kill @e[tag=vfp_test]')
                report['checks']['entity_spawn_and_combat']=True
            else:
                wait_value('Mob damage',lambda:ctl('entities','8',identifier),
                           lambda value:any(entity['id']==entity_id and entity['health']<20
                                            for entity in value.get('entities',[])))
                report['checks']['entity_spawn_and_combat']=True
            if protocol<393:
                console('gamerule keepInventory true')
            if protocol<47:
                ctl('chat','/kill')
            else:
                console('kill VfpSmoke')
            wait_value('Death screen',lambda:ctl('state'),lambda s:s['screen']=='dead')
            ctl('respawn')
            wait_value('Respawn',lambda:ctl('state'),lambda s:s['screen']=='playing' and s['player']['health']>0)
            report['checks']['death_and_respawn']=True
            gamemode('creative')
            if protocol>=393:
                console('execute in minecraft:the_nether run tp VfpSmoke 0 80 0')
                wait_value('Nether travel',lambda:ctl('status'),lambda s:s['dimension']=='minecraft:the_nether' and s['server']['loaded'])
                return_y = -60 if protocol>=757 else 5
                console(f'execute in minecraft:overworld run tp VfpSmoke 0 {return_y} 0')
                wait_value('Overworld return',lambda:ctl('status'),lambda s:s['dimension']=='minecraft:overworld' and s['server']['loaded'])
                report['checks']['dimension_changes']=True
            else:
                bx,by,bz=ctl('state')['player']['block_pos']
                bz+=2
                for dy in (-1,3):
                    for dx in (-1,0,1,2):
                        console(f'setblock {bx+dx} {by+dy} {bz} minecraft:obsidian')
                for dy in range(3):
                    for dx in (-1,2):
                        console(f'setblock {bx+dx} {by+dy} {bz} minecraft:obsidian')
                console(f'setblock {bx} {by} {bz} minecraft:fire')
                check_block(bx,by,bz,'portal','vfp_portal_placed')
                wait_value('Client portal block',lambda:ctl('block',str(bx),str(by),str(bz)),
                           lambda block:block.get('block')=='minecraft:nether_portal')
                ctl('look','0','0')
                ctl('move','forward','0.75')
                wait_value('Nether portal travel',lambda:ctl('status'),
                           lambda s:s['dimension']=='minecraft:the_nether' and s['server']['loaded'],seconds=40)
                report['checks']['nether_portal']=True
            final=ctl('status')
            assert final['server']['decode_errors']==0,final['server']
            (run/'final-status.json').write_text(json.dumps(final,indent=2),encoding='utf-8')
            ctl('disconnect')
            wait_value('Disconnect',lambda:ctl('state'),lambda s:s['screen']=='title')
            time.sleep(.5)
            if args.auto:
                reconnect_port=port
            else:
                (run/'reconnect').mkdir()
                reconnect=TranscriptProxy(port,run/'reconnect',protocol);proxies.append(reconnect)
                reconnect_port=reconnect.port
            ctl('connect',f'127.0.0.1:{reconnect_port}')
            wait_value('Reconnect world entry',lambda:ctl('state'),
                       lambda s:s['screen']=='playing' and s.get('player') is not None,seconds=45)
            rejoined=wait_value('Reconnect chunks',lambda:ctl('status'),
                               lambda s:s['server'] and s['server']['loaded'] and s['chunks']>=9,seconds=45)
            assert rejoined['server']['decode_errors']==0,rejoined['server']
            report['checks']['reconnect']=True
            if protocol<393:
                wait_value('Server saved inventory move',lambda:hotbar_has('minecraft:stone',destination_slot),bool)
                report['checks']['inventory_clicks']=True
                saved=ctl('inventory')['counts']
                assert saved.get('minecraft:crafting_table',0)>=1,saved
                report['checks']['crafting']=True
                assert saved.get('minecraft:diamond',0)>=3,saved
                report['checks']['container_transfer']=True
            (run/'reconnect-status.json').write_text(json.dumps(rejoined,indent=2),encoding='utf-8')
        ctl('screenshot',str(run/'world.png'))
        print('World entered: '+args.release,flush=True)
    except Exception as error:
        report['error']=str(error) or repr(error)
    finally:
        if 'client' in locals() and client.poll() is None:
            try:
                ctl('state')
                (run/'screen.json').write_text(json.dumps(ctl('screen'),indent=2),encoding='utf-8')
                ctl('quit');client.wait(timeout=20)
            except Exception:
                client.terminate()
                try: client.wait(timeout=10)
                except subprocess.TimeoutExpired: client.kill();client.wait()
        if 'server' in locals() and server.poll() is None:
            try: console('stop');server.wait(timeout=30)
            except Exception: server.kill();server.wait()
        if 'origin' in locals() and origin.poll() is None:
            try: origin.stdin.write(b'stop\n');origin.stdin.flush();origin.wait(timeout=30)
            except Exception: origin.kill();origin.wait()
        for proxy in proxies: proxy.close()
        for handle in log_handles: handle.close()
        report['processes_stopped']=all(p.poll() is not None for p in processes)
        (run/'result.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
        print(json.dumps(report,indent=2),flush=True)
    if report['error']:
        raise SystemExit(1)


if __name__=='__main__':
    main()
