// Thin client over the Mira daemon's D-Bus surface (ro.novabytecode.Mira).
//
// The extension holds no business logic: it reads cached properties and reacts
// to signals. MiraClient re-reads on `g-properties-changed`, forwards the
// daemon's custom signals, and reports daemon availability via g-name-owner.

import Gio from 'gi://Gio';
import GLib from 'gi://GLib';

export const BUS_NAME = 'ro.novabytecode.Mira';
export const OBJECT_PATH = '/ro/novabytecode/Mira';

const IFACE = {
    service: 'ro.novabytecode.Mira.Service',
    media: 'ro.novabytecode.Mira.Media',
    lyrics: 'ro.novabytecode.Mira.Lyrics',
    power: 'ro.novabytecode.Mira.Power',
    system: 'ro.novabytecode.Mira.System',
    pomodoro: 'ro.novabytecode.Mira.Pomodoro',
    water: 'ro.novabytecode.Mira.Water',
    todo: 'ro.novabytecode.Mira.Todo',
    shelf: 'ro.novabytecode.Mira.Shelf',
};

export class MiraClient {
    constructor() {
        this._handlers = new Map(); // event -> Set<callback>
        this._proxies = {};
        this._signalIds = [];
    }

    // --- tiny event emitter ---
    connect(event, cb) {
        if (!this._handlers.has(event))
            this._handlers.set(event, new Set());
        this._handlers.get(event).add(cb);
    }

    _emit(event, ...args) {
        const set = this._handlers.get(event);
        if (!set)
            return;
        for (const cb of set) {
            try {
                cb(...args);
            } catch (e) {
                logError(e, `mira: handler for ${event}`);
            }
        }
    }

    get available() {
        const p = this._proxies.service;
        return !!(p && p.g_name_owner);
    }

    start() {
        const bus = Gio.DBus.session;
        for (const [key, name] of Object.entries(IFACE)) {
            const proxy = Gio.DBusProxy.new_sync(
                bus,
                Gio.DBusProxyFlags.NONE,
                null,
                BUS_NAME,
                OBJECT_PATH,
                name,
                null
            );
            this._proxies[key] = proxy;
        }

        // Availability: watch the service proxy's owner.
        this._signalIds.push([
            this._proxies.service,
            this._proxies.service.connect('notify::g-name-owner', () =>
                this._emit('availability-changed', this.available)
            ),
        ]);

        // Property changes per interface -> coarse "changed" events.
        this._watchProps('media', 'media-changed');
        this._watchProps('power', 'power-changed');
        this._watchProps('lyrics', 'lyrics-props-changed');
        this._watchProps('system', 'system-changed');
        this._watchProps('pomodoro', 'pomodoro-changed');
        this._watchProps('water', 'water-changed');

        // Custom signals.
        this._watchSignals();

        // Emit an initial snapshot.
        this._emit('availability-changed', this.available);
        this._emit('media-changed');
        this._emit('power-changed');
        this._emit('pomodoro-changed');
        this._emit('water-changed');
        this._emit('todo-changed');
        this._emit('shelf-changed');
    }

    _watchProps(key, event) {
        const proxy = this._proxies[key];
        this._signalIds.push([
            proxy,
            proxy.connect('g-properties-changed', () => this._emit(event)),
        ]);
    }

    _watchSignals() {
        const lyrics = this._proxies.lyrics;
        this._signalIds.push([
            lyrics,
            lyrics.connect('g-signal', (_p, _sender, signal, params) => {
                if (signal === 'LyricsChanged') {
                    this._emit('lyrics-changed');
                } else if (signal === 'ActiveLineChanged') {
                    const idx = params.deepUnpack()[0];
                    this._emit('active-line-changed', Number(idx));
                }
            }),
        ]);

        const power = this._proxies.power;
        this._signalIds.push([
            power,
            power.connect('g-signal', (_p, _sender, signal, params) => {
                if (signal === 'ChargerConnected') {
                    this._emit('alert', 'charger-connected', Number(params.deepUnpack()[0]));
                } else if (signal === 'ChargerDisconnected') {
                    this._emit('alert', 'charger-disconnected', Number(params.deepUnpack()[0]));
                }
            }),
        ]);

        const pomodoro = this._proxies.pomodoro;
        this._signalIds.push([
            pomodoro,
            pomodoro.connect('g-signal', (_p, _sender, signal, params) => {
                if (signal === 'PhaseElapsed')
                    this._emit('alert', 'pomodoro-phase', params.deepUnpack()[0]);
            }),
        ]);

        const todo = this._proxies.todo;
        this._signalIds.push([
            todo,
            todo.connect('g-signal', (_p, _sender, signal, params) => {
                if (signal === 'ItemsChanged') {
                    this._emit('todo-changed');
                } else if (signal === 'ReminderDue') {
                    const [, text] = params.deepUnpack();
                    this._emit('alert', 'reminder', text);
                }
            }),
        ]);

        const shelf = this._proxies.shelf;
        this._signalIds.push([
            shelf,
            shelf.connect('g-signal', (_p, _sender, signal, _params) => {
                if (signal === 'ItemsChanged')
                    this._emit('shelf-changed');
            }),
        ]);
    }

    // --- property readers (safe defaults when daemon is absent) ---
    _get(key, prop, fallback) {
        const proxy = this._proxies[key];
        if (!proxy)
            return fallback;
        const v = proxy.get_cached_property(prop);
        return v === null ? fallback : v.unpack();
    }

    media() {
        return {
            status: this._get('media', 'Status', 'Stopped'),
            title: this._get('media', 'Title', ''),
            artist: this._get('media', 'Artist', ''),
            album: this._get('media', 'Album', ''),
            artUrl: this._get('media', 'ArtUrl', ''),
            lengthMicros: Number(this._get('media', 'LengthMicros', 0)),
            positionMicros: Number(this._get('media', 'PositionMicros', 0)),
            canGoNext: this._get('media', 'CanGoNext', false),
            canGoPrevious: this._get('media', 'CanGoPrevious', false),
            canSeek: this._get('media', 'CanSeek', false),
        };
    }

    power() {
        return {
            hasBattery: this._get('power', 'HasBattery', false),
            percentage: Number(this._get('power', 'Percentage', 0)),
            state: this._get('power', 'State', 'Unknown'),
            onAc: this._get('power', 'OnAc', false),
        };
    }

    lyricsMeta() {
        return {
            available: this._get('lyrics', 'Available', false),
            synced: this._get('lyrics', 'Synced', false),
            source: this._get('lyrics', 'Source', 'none'),
        };
    }

    pomodoro() {
        return {
            phase: this._get('pomodoro', 'Phase', 'work'),
            remaining: Number(this._get('pomodoro', 'RemainingSecs', 0)),
            running: this._get('pomodoro', 'Running', false),
            cycles: Number(this._get('pomodoro', 'CompletedCycles', 0)),
        };
    }

    water() {
        return {
            glasses: Number(this._get('water', 'GlassesToday', 0)),
            goal: Number(this._get('water', 'Goal', 8)),
        };
    }

    // Fetch todos asynchronously; cb receives [[id, text, done, remind], ...].
    getTodos(cb) {
        this._callRet('todo', 'GetItems', null, ret => {
            const items = ret.deepUnpack()[0].map(([id, text, done, remind]) => [
                Number(id),
                text,
                done,
                Number(remind),
            ]);
            cb(items);
        }, () => cb([]));
    }

    // Fetch shelf items asynchronously; cb receives [[id, path, epoch], ...].
    getShelf(cb) {
        this._callRet('shelf', 'GetItems', null, ret => {
            const items = ret.deepUnpack()[0].map(([id, path, epoch]) => [
                Number(id),
                path,
                Number(epoch),
            ]);
            cb(items);
        }, () => cb([]));
    }

    // Fetch all lyric lines asynchronously; cb receives [[micros, text], ...].
    getLines(cb) {
        const proxy = this._proxies.lyrics;
        if (!proxy) {
            cb([]);
            return;
        }
        proxy.call('GetLines', null, Gio.DBusCallFlags.NONE, -1, null, (p, res) => {
            try {
                const ret = p.call_finish(res);
                const lines = ret.deepUnpack()[0].map(([t, text]) => [Number(t), text]);
                cb(lines);
            } catch (e) {
                logError(e, 'mira: GetLines');
                cb([]);
            }
        });
    }

    // --- control methods ---
    _call(key, method, params = null) {
        const proxy = this._proxies[key];
        if (!proxy)
            return;
        proxy.call(method, params, Gio.DBusCallFlags.NONE, -1, null, (p, res) => {
            try {
                p.call_finish(res);
            } catch (e) {
                logError(e, `mira: ${method}`);
            }
        });
    }

    // Like _call but delivers the return value (or calls onErr).
    _callRet(key, method, params, onOk, onErr) {
        const proxy = this._proxies[key];
        if (!proxy) {
            onErr && onErr();
            return;
        }
        proxy.call(method, params, Gio.DBusCallFlags.NONE, -1, null, (p, res) => {
            try {
                onOk(p.call_finish(res));
            } catch (e) {
                logError(e, `mira: ${method}`);
                onErr && onErr();
            }
        });
    }

    // Pomodoro
    pomoToggle() {
        this._call('pomodoro', 'Toggle');
    }
    pomoReset() {
        this._call('pomodoro', 'Reset');
    }
    pomoSkip() {
        this._call('pomodoro', 'Skip');
    }

    // Water
    waterAdd() {
        this._call('water', 'Add');
    }
    waterRemove() {
        this._call('water', 'Remove');
    }

    // Todo
    todoAdd(text) {
        this._call('todo', 'Add', new GLib.Variant('(s)', [text]));
    }
    todoToggle(id) {
        this._call('todo', 'Toggle', new GLib.Variant('(x)', [id]));
    }
    todoRemove(id) {
        this._call('todo', 'Remove', new GLib.Variant('(x)', [id]));
    }
    todoSetReminder(id, epoch) {
        this._call('todo', 'SetReminder', new GLib.Variant('(xx)', [id, epoch]));
    }

    // Shelf
    shelfAdd(path) {
        this._call('shelf', 'Add', new GLib.Variant('(s)', [path]));
    }
    shelfRemove(id) {
        this._call('shelf', 'Remove', new GLib.Variant('(x)', [id]));
    }

    playPause() {
        this._call('media', 'PlayPause');
    }
    next() {
        this._call('media', 'Next');
    }
    previous() {
        this._call('media', 'Previous');
    }
    setPosition(micros) {
        this._call('media', 'SetPosition', new GLib.Variant('(x)', [micros]));
    }
    seekToLine(index) {
        this._call('lyrics', 'SeekToLine', new GLib.Variant('(u)', [index]));
    }

    destroy() {
        for (const [obj, id] of this._signalIds)
            obj.disconnect(id);
        this._signalIds = [];
        this._proxies = {};
        this._handlers.clear();
    }
}
