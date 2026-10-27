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
        print('D-Bus reminders: validation, due signal, restart, snooze, acknowledgement and task isolation passed')
    finally:
        stop(daemon)
        monitor.terminate()
        output, _ = monitor.communicate(timeout=5)
    assert output.count('member=Due') == 2, output
    print('Exactly two Due signals: initial expiry and snooze; no duplicate on restart')
