"""Run with: dbus-run-session -- python3 crates/mira-daemon/tests/reminders_dbus.py.
Uses an isolated database; requires cargo build -p mira-daemon and busctl.
"""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[3]
NAME = 'ro.novabytecode.Mira'
PATH = '/ro/novabytecode/Mira'
IFACE = NAME + '.Reminders'


def call(method, signature='', *args, iface=IFACE, check=True):
    result = subprocess.run(['busctl', '--user', '--auto-start=no', '--json=short', 'call', NAME, PATH,
                             iface, method, *([signature, *map(str, args)] if signature else [])],
                            capture_output=True, text=True, check=check)
    if not check:
        return result.returncode
    return json.loads(result.stdout)['data'][0] if result.stdout.strip() else None


def start(env):
    p = subprocess.Popen([str(ROOT / 'target/debug/mira-daemon')], env=env,
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    for _ in range(50):
        if call('Ping', iface=NAME + '.Service', check=False) == 0:
            return p
        if p.poll() is not None:
            raise RuntimeError('daemon exited during startup')
        time.sleep(.1)
    p.terminate()
    raise RuntimeError('daemon did not acquire its bus name')


def stop(p):
    p.terminate()
    p.wait(timeout=5)
    for _ in range(50):
        if call('Ping', iface=NAME + '.Service', check=False) != 0:
            return
        time.sleep(.05)


def wait_due(id):
    for _ in range(60):
        rows = call('GetItems')
        if any(row[0] == id and row[4] for row in rows):
            return rows
        time.sleep(.1)
    raise AssertionError('Reminder was not marked delivered')


with tempfile.TemporaryDirectory(prefix='mira-reminders-') as tmp:
    env = {**os.environ, 'XDG_DATA_HOME': tmp, 'XDG_CONFIG_HOME': tmp}
    monitor = subprocess.Popen(['dbus-monitor', '--session', f"type='signal',interface='{IFACE}',member='Due'"],
                               stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
    daemon = start(env)
    try:
        assert call('Lookup', 'ss', '', '', iface=NAME + '.Lyrics', check=False) != 0
        task = call('Add', 's', 'Independent task', iface=NAME + '.Todo')
        assert call('Add', 'sx', '', int(time.time()) + 60, check=False) != 0
        id = call('Add', 'sx', 'Call Mom', int(time.time()) + 2)
        rows = wait_due(id)
        assert rows[0][1] == 'Call Mom'
        assert len(call('GetItems', iface=NAME + '.Todo')) == 1
        stop(daemon)
        daemon = start(env)
        assert call('GetItems')[0][4] is True
        time.sleep(1.1)
        call('Snooze', 'xx', id, int(time.time()) + 2)
        assert call('GetItems')[0][4] is False
        wait_due(id)
        call('Remove', 'x', id)
        assert call('GetItems') == []
        assert call('GetItems', iface=NAME + '.Todo')[0][0] == task
        recurring = call('AddNatural', 's', 'Raport în fiecare zi la 9')
        details = json.loads(call('GetDetails'))
        original = next(r for r in details if r['id'] == recurring)
        assert original['recurrence']['kind'] == 'daily'
        call('CompleteOccurrence', 'xx', recurring, original['due_epoch'])
        changed = json.loads(call('GetDetails'))[0]
        assert changed['due_epoch'] > original['due_epoch']
        call('CompleteOccurrence', 'xx', recurring, original['due_epoch'])
        assert json.loads(call('GetDetails'))[0]['due_epoch'] == changed['due_epoch']
        call('Toggle', 'x', task, iface=NAME + '.Todo')
        summary = json.loads(call('GetToday', iface=NAME + '.Summary'))
        assert summary['todos_completed'] == 1 and summary['reminders_completed'] == 1
        preview = json.loads(call('PreviewNatural', 's', 'Sună-l pe Alex peste 20 de minute'))
        assert preview['valid'] and preview['draft']['title'] == 'Sună-l pe Alex'
        invalid = json.loads(call('PreviewNatural', 's', 'Test la 24:00'))
        assert not invalid['valid']
        call('Remove', 'x', recurring)
        print('D-Bus companion: natural parsing, recurrence, idempotent completion, summary, expiry and restart passed')
    finally:
        stop(daemon)
        monitor.terminate()
        output, _ = monitor.communicate(timeout=5)
    assert output.count('member=Due') == 2, output
    print('Exactly two Due signals: initial expiry and snooze; no duplicate on restart')
