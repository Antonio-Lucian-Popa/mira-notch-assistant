"""Integration: dbus-run-session -- python3 crates/mira-daemon/tests/workspace_dbus.py"""
import json, os, subprocess, tempfile, time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
NAME='ro.novabytecode.Mira'; IFACE=NAME+'.Workspace'; OBJECT='/ro/novabytecode/Mira'
def call(method, signature='', *args):
    p=subprocess.run(['busctl','--user','--auto-start=no','--json=short','call',NAME,OBJECT,IFACE,method,*([signature,*map(str,args)] if signature else [])],capture_output=True,text=True,check=True)
    if not p.stdout.strip(): return None
    data=json.loads(p.stdout)['data']
    return json.loads(data[0]) if data else None
with tempfile.TemporaryDirectory(prefix='mira-workspace-') as tmp:
    folder=Path(tmp)/'folder';folder.mkdir();(folder/'note.txt').write_text('Hello [Mira]\nSearch the literal brackets.\n')
    env={**os.environ,'XDG_DATA_HOME':tmp+'/data','XDG_CONFIG_HOME':tmp+'/config'}
    def start():
        daemon=subprocess.Popen([str(ROOT/'target/debug/mira-daemon')],env=env,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        for _ in range(60):
            try: call('Snapshot');return daemon
            except subprocess.CalledProcessError: time.sleep(.1)
        raise AssertionError('daemon startup failed')
    daemon=start()
    try:
        result=call('AddPaths','as',1,str(folder));assert len(result['links'])==2
        call('Note','s','Sticky\nsecond line');call('PushClip','s','first');call('PushClip','s','first')
        state=call('Snapshot');assert state['note']=='Sticky\nsecond line' and state['clips']==['first']
        call('Capture','b','false');call('PushClip','s','must not be captured');assert call('Snapshot')['clips']==['first']
        result=call('Search','s','[mira]');assert len(result['hits'])==1 and result['hits'][0]['line']==1
        call('Clear');state=call('Snapshot');assert state['links']==[] and state['clips']==[] and state['note']==''
        assert (folder/'note.txt').exists()
        call('Note','s','discard after restart');daemon.terminate();daemon.wait(timeout=5);daemon=start()
        assert call('Snapshot')['note']==''
        print('Workspace D-Bus: folder references, literal grep, clipboard pause/dedup, sticky, safe clear and session reset passed')
    finally: daemon.terminate();daemon.wait(timeout=5)
