// The notch widget: a centered top pill that expands on hover and shows the
// music and lyrics pages. Geometry and animation timings come from design
// tokens so it matches the Windows notch. All state is pushed in from the
// daemon via MiraClient; this file only draws and animates.

import Clutter from 'gi://Clutter';
import GLib from 'gi://GLib';
import GObject from 'gi://GObject';
import St from 'gi://St';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';

const PAGES = ['music', 'lyrics', 'focus', 'water', 'shelf', 'todo'];
const PAGE_LABELS = {
    focus: 'Focus',
    water: 'Apă · Water',
    shelf: 'Raft · Shelf',
    todo: 'To-do',
};

export const Notch = GObject.registerClass(
class Notch extends St.Bin {
    _init(client, tokens, settings) {
        super._init({
            style_class: 'mira-notch',
            reactive: true,
            track_hover: true,
            can_focus: true,
            clip_to_allocation: true,
            x_expand: false,
            y_expand: false,
        });

        this._client = client;
        this._tokens = tokens;
        this._settings = settings;

        this._page = 0;
        this._open = false;
        this._activeLine = -1;
        this._lines = [];
        this._hasTrack = false;
        this._playing = false;
        this._closeTimer = 0;
        this._alertTimer = 0;
        this._clockTimer = 0;
        this._vizTimer = 0;
        // Mascot (resting eyes), off by default via the `mascot` setting.
        this._mascotActive = false;
        this._idleTimer = 0;
        this._blinkTimer = 0;
        this._sleepTimer = 0;

        this._buildContent();
        this._computeSizes();
        this.set_size(this._closed.w, this._closed.h);
        this._reposition(this._closed.w);

        this.connect('notify::hover', () => this._onHover());
        this.connect('scroll-event', (_a, ev) => this._onScroll(ev));
        this.connect('key-press-event', (_a, ev) => this._onKey(ev));
        this.connect('destroy', () => this._onDestroy());

        this._startClock();
        this._refreshMedia();
        this._armIdle();
    }

    // --- layout construction ---
    _buildContent() {
        this._stack = new St.Widget({
            layout_manager: new Clutter.BinLayout(),
            x_expand: true,
            y_expand: true,
        });
        this.set_child(this._stack);

        // Closed view: clock, or mini music (art dot + visualizer).
        this._closedBox = new St.BoxLayout({
            style_class: 'mira-closed',
            x_align: Clutter.ActorAlign.CENTER,
            y_align: Clutter.ActorAlign.CENTER,
            x_expand: true,
            y_expand: true,
        });
        this._clock = new St.Label({style_class: 'mira-clock', y_align: Clutter.ActorAlign.CENTER});
        this._viz = this._buildVisualizer();
        this._eyes = this._buildEyes();
        this._closedBox.add_child(this._clock);
        this._closedBox.add_child(this._viz);
        this._closedBox.add_child(this._eyes);
        this._viz.hide();
        this._eyes.hide();

        // Open view: a page area plus a dots row.
        this._openBox = new St.BoxLayout({
            vertical: true,
            style_class: 'mira-open',
            x_expand: true,
            y_expand: true,
        });
        this._pageArea = new St.Widget({
            layout_manager: new Clutter.BinLayout(),
            x_expand: true,
            y_expand: true,
        });
        this._dots = new St.BoxLayout({
            style_class: 'mira-dots',
            x_align: Clutter.ActorAlign.CENTER,
        });
        this._buildDots();
        this._openBox.add_child(this._pageArea);
        this._openBox.add_child(this._dots);
        this._openBox.hide();

        // Alert view.
        this._alertBox = new St.BoxLayout({style_class: 'mira-alert', x_align: Clutter.ActorAlign.CENTER, y_align: Clutter.ActorAlign.CENTER});
        this._alertIcon = new St.Icon({style_class: 'mira-alert-icon', icon_size: 18});
        this._alertLabel = new St.Label({style_class: 'mira-alert-label', y_align: Clutter.ActorAlign.CENTER});
        this._alertBox.add_child(this._alertIcon);
        this._alertBox.add_child(this._alertLabel);
        this._alertBox.hide();

        this._stack.add_child(this._closedBox);
        this._stack.add_child(this._openBox);
        this._stack.add_child(this._alertBox);

        this._buildMusicPage();
        this._buildLyricsPage();
        this._buildPlaceholderPages();
        this._showPageActor(0);
    }

    _buildVisualizer() {
        const box = new St.BoxLayout({style_class: 'mira-viz', y_align: Clutter.ActorAlign.CENTER});
        this._vizBars = [];
        const n = this._tokens.visualizer?.bars ?? 5;
        for (let i = 0; i < n; i++) {
            const bar = new St.Widget({style_class: 'mira-viz-bar'});
            bar.set_size(3, this._tokens.visualizer?.minHeight ?? 3);
            this._vizBars.push(bar);
            box.add_child(bar);
        }
        return box;
    }

    _buildEyes() {
        const box = new St.BoxLayout({
            style_class: 'mira-eyes',
            x_align: Clutter.ActorAlign.CENTER,
            y_align: Clutter.ActorAlign.CENTER,
        });
        this._eyeActors = [];
        this._eyeH = 9;
        for (let i = 0; i < 2; i++) {
            const eye = new St.Widget({style_class: 'mira-eye'});
            eye.set_size(9, this._eyeH);
            this._eyeActors.push(eye);
            box.add_child(eye);
        }
        return box;
    }

    _buildDots() {
        this._dotActors = [];
        for (let i = 0; i < PAGES.length; i++) {
            const dot = new St.Button({style_class: 'mira-dot'});
            dot.connect('clicked', () => this.setPage(i));
            this._dotActors.push(dot);
            this._dots.add_child(dot);
        }
    }

    _buildMusicPage() {
        const page = new St.BoxLayout({style_class: 'mira-page mira-page-music', x_expand: true});
        this._art = new St.Bin({style_class: 'mira-art'});
        const info = new St.BoxLayout({vertical: true, x_expand: true, y_align: Clutter.ActorAlign.CENTER});
        this._title = new St.Label({style_class: 'mira-title'});
        this._artist = new St.Label({style_class: 'mira-artist'});
        info.add_child(this._title);
        info.add_child(this._artist);

        const controls = new St.BoxLayout({style_class: 'mira-controls', y_align: Clutter.ActorAlign.CENTER});
        this._btnPrev = this._iconButton('media-skip-backward-symbolic', () => this._client.previous());
        this._btnPlay = this._iconButton('media-playback-start-symbolic', () => this._client.playPause());
        this._btnNext = this._iconButton('media-skip-forward-symbolic', () => this._client.next());
        controls.add_child(this._btnPrev);
        controls.add_child(this._btnPlay);
        controls.add_child(this._btnNext);

        page.add_child(this._art);
        page.add_child(info);
        page.add_child(controls);
        this._musicPage = page;
        this._pageArea.add_child(page);
    }

    _buildLyricsPage() {
        const page = new St.BoxLayout({
            vertical: true,
            style_class: 'mira-page mira-page-lyrics',
            x_expand: true,
            y_expand: true,
        });
        // Three rolling lines: previous, current, next.
        this._lyricLabels = [];
        for (let i = 0; i < 3; i++) {
            const btn = new St.Button({style_class: 'mira-lyric-line', x_align: Clutter.ActorAlign.CENTER});
            const label = new St.Label();
            btn.set_child(label);
            btn._label = label;
            btn.connect('clicked', () => this._onLyricClicked(btn));
            this._lyricLabels.push(btn);
            page.add_child(btn);
        }
        this._lyricsPage = page;
        this._pageArea.add_child(page);
    }

    _buildPlaceholderPages() {
        this._placeholders = {};
        for (const name of ['focus', 'water', 'shelf', 'todo']) {
            const page = new St.BoxLayout({style_class: 'mira-page mira-page-soon', x_align: Clutter.ActorAlign.CENTER, y_align: Clutter.ActorAlign.CENTER, x_expand: true, y_expand: true});
            const label = new St.Label({style_class: 'mira-soon', text: `${PAGE_LABELS[name]}`});
            page.add_child(label);
            this._placeholders[name] = page;
            this._pageArea.add_child(page);
        }
    }

    _iconButton(icon, onClick) {
        const btn = new St.Button({style_class: 'mira-ctl-btn', can_focus: true});
        btn.set_child(new St.Icon({icon_name: icon, icon_size: 16}));
        btn.connect('clicked', onClick);
        return btn;
    }

    // --- sizing / positioning ---
    _computeSizes() {
        const t = this._tokens.notch;
        const scale = this._sizeScale();
        const minimal = this._settings.get_boolean('minimal');
        this._closed = minimal
            ? {w: t.minimal.width * scale, h: t.minimal.height, r: t.minimal.radiusBottom}
            : {w: t.closed.width * scale, h: t.closed.height, r: t.closed.radiusBottom};
        this._hoverSize = {w: t.hover.width * scale, h: t.hover.height, r: t.hover.radiusBottom};
        this._alertSize = {w: t.alert.width * scale, h: t.alert.height, r: t.alert.radiusBottom};
    }

    _sizeScale() {
        const monitor = Main.layoutManager.primaryMonitor;
        const s = this._tokens.sizing ?? {};
        const auto = monitor && monitor.width >= (s.wideMonitorMinWidthPx ?? 2560)
            ? (s.wideScaleFactor ?? 1.25)
            : 1.0;
        const manual = this._settings.get_double('size-scale');
        return auto * manual;
    }

    _reposition(width) {
        const monitor = Main.layoutManager.primaryMonitor;
        if (!monitor)
            return;
        const x = Math.round(monitor.x + (monitor.width - width) / 2);
        this.set_position(x, monitor.y);
    }

    // --- open / close ---
    _onHover() {
        if (this.hover)
            this.open();
        else
            this._scheduleClose();
    }

    open() {
        if (this._closeTimer) {
            GLib.source_remove(this._closeTimer);
            this._closeTimer = 0;
        }
        if (this._open)
            return;
        this._open = true;
        this._cancelIdle();
        this._closedBox.hide();
        this._alertBox.hide();
        this._openBox.show();
        this.grab_key_focus();
        this._animateTo(this._hoverSize, this._tokens.motion.open.durationMs, Clutter.AnimationMode.EASE_OUT_BACK);
        this._refreshLyrics();
    }

    _scheduleClose() {
        if (this._closeTimer)
            GLib.source_remove(this._closeTimer);
        const delay = this._tokens.notch.hoverCloseDelayMs ?? 300;
        this._closeTimer = GLib.timeout_add(GLib.PRIORITY_DEFAULT, delay, () => {
            this._closeTimer = 0;
            this.close();
            return GLib.SOURCE_REMOVE;
        });
    }

    close() {
        if (!this._open)
            return;
        this._open = false;
        this._animateTo(this._closed, this._tokens.motion.close.durationMs, Clutter.AnimationMode.EASE_OUT_QUAD, () => {
            if (!this._open) {
                this._openBox.hide();
                this._closedBox.show();
                this._updateClosedVisual();
                this._armIdle();
            }
        });
    }

    _animateTo(size, duration, mode, onComplete) {
        this.remove_all_transitions();
        const monitor = Main.layoutManager.primaryMonitor;
        const x = monitor ? Math.round(monitor.x + (monitor.width - size.w) / 2) : this.x;
        this.set_style(`border-bottom-left-radius:${size.r}px;border-bottom-right-radius:${size.r}px;`);
        this.ease({
            x,
            width: size.w,
            height: size.h,
            duration,
            mode,
            onComplete: () => onComplete && onComplete(),
        });
    }

    // --- navigation ---
    setPage(i) {
        const n = PAGES.length;
        this._page = ((i % n) + n) % n;
        this._showPageActor(this._page);
        this._dotActors.forEach((d, idx) =>
            idx === this._page ? d.add_style_class_name('active') : d.remove_style_class_name('active')
        );
        if (PAGES[this._page] === 'lyrics')
            this._refreshLyrics();
    }

    _showPageActor(i) {
        const name = PAGES[i];
        this._musicPage.visible = name === 'music';
        this._lyricsPage.visible = name === 'lyrics';
        for (const [key, actor] of Object.entries(this._placeholders))
            actor.visible = name === key;
    }

    _onScroll(ev) {
        const dir = ev.get_scroll_direction();
        if (dir === Clutter.ScrollDirection.UP || dir === Clutter.ScrollDirection.LEFT)
            this.setPage(this._page - 1);
        else if (dir === Clutter.ScrollDirection.DOWN || dir === Clutter.ScrollDirection.RIGHT)
            this.setPage(this._page + 1);
        return Clutter.EVENT_STOP;
    }

    _onKey(ev) {
        const sym = ev.get_key_symbol();
        if (sym === Clutter.KEY_Left)
            this.setPage(this._page - 1);
        else if (sym === Clutter.KEY_Right)
            this.setPage(this._page + 1);
        else
            return Clutter.EVENT_PROPAGATE;
        return Clutter.EVENT_STOP;
    }

    // --- media / lyrics / power updates (called by extension) ---
    _refreshMedia() {
        const m = this._client.media();
        const playing = m.status === 'Playing';
        const hasTrack = m.title !== '' || m.artist !== '';

        this._title.text = m.title || '—';
        this._artist.text = m.artist || '';
        this._btnPlay.get_child().icon_name = playing
            ? 'media-playback-pause-symbolic'
            : 'media-playback-start-symbolic';
        this._btnNext.reactive = m.canGoNext;
        this._btnPrev.reactive = m.canGoPrevious;

        this._hasTrack = hasTrack;
        this._playing = playing;
        // Actively playing music counts as activity: keep the mascot away.
        // A merely *paused* track (browsers keep an MPRIS session around) must
        // not block resting, so we gate on `playing`, not `hasTrack`.
        if (playing)
            this._resetIdle();
        this._updateClosedVisual();
        if (playing)
            this._startVisualizer();
        else
            this._stopVisualizer();
    }

    // Decide what the closed notch shows: mascot eyes > visualizer > clock.
    _updateClosedVisual() {
        const minimal = this._settings.get_boolean('minimal');
        const showEyes = this._mascotActive && !minimal;
        // Visualizer only while actually playing, not for a paused track.
        const showViz = !showEyes && this._playing && !minimal;
        this._eyes.visible = showEyes;
        this._viz.visible = showViz;
        this._clock.visible = !showEyes && !showViz;
    }

    refreshMedia() {
        this._refreshMedia();
    }

    refreshPower() {
        // Power state currently only drives alerts; reserved for a battery pip.
    }

    _refreshLyrics() {
        if (PAGES[this._page] !== 'lyrics' && !this._open)
            return;
        this._client.getLines(lines => {
            this._lines = lines;
            this._renderLyrics();
        });
    }

    onLyricsChanged() {
        this._activeLine = -1;
        this._refreshLyrics();
    }

    onActiveLine(index) {
        this._activeLine = index;
        this._renderLyrics();
    }

    _renderLyrics() {
        const cur = this._activeLine;
        const rows = [cur - 1, cur, cur + 1];
        this._lyricLabels.forEach((btn, i) => {
            const idx = rows[i];
            const line = idx >= 0 && idx < this._lines.length ? this._lines[idx] : null;
            btn._lineIndex = idx;
            btn._label.text = line ? line[1] : '';
            if (i === 1)
                btn.add_style_class_name('active');
            else
                btn.remove_style_class_name('active');
            btn.reactive = !!line;
        });
    }

    _onLyricClicked(btn) {
        if (typeof btn._lineIndex === 'number' && btn._lineIndex >= 0)
            this._client.seekToLine(btn._lineIndex);
    }

    // --- alerts ---
    showAlert(type, percentage) {
        const map = {
            'charger-connected': ['battery-full-charging-symbolic', `⚡ ${Math.round(percentage)}%`],
            'charger-disconnected': ['battery-symbolic', `🔋 ${Math.round(percentage)}%`],
        };
        const [icon, text] = map[type] ?? ['dialog-information-symbolic', ''];
        this._alertIcon.icon_name = icon;
        this._alertLabel.text = text;

        this._cancelIdle();
        this._closedBox.hide();
        this._openBox.hide();
        this._alertBox.show();
        this._animateTo(this._alertSize, this._tokens.motion.alertIn.durationMs, Clutter.AnimationMode.EASE_OUT_BACK);

        if (this._alertTimer)
            GLib.source_remove(this._alertTimer);
        const hold = this._tokens.motion.alertHoldMs ?? 2600;
        this._alertTimer = GLib.timeout_add(GLib.PRIORITY_DEFAULT, hold, () => {
            this._alertTimer = 0;
            this._alertBox.hide();
            if (!this.hover) {
                this._closedBox.show();
                this._updateClosedVisual();
                this._animateTo(this._closed, this._tokens.motion.alertOut.durationMs, Clutter.AnimationMode.EASE_OUT_QUAD);
                this._armIdle();
            } else {
                this.open();
            }
            return GLib.SOURCE_REMOVE;
        });
    }

    // --- clock ---
    _startClock() {
        this._updateClock();
        this._scheduleClockTick();
    }

    _scheduleClockTick() {
        const now = GLib.DateTime.new_now_local();
        const secsToNextMinute = 60 - now.get_second();
        this._clockTimer = GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT, secsToNextMinute, () => {
            this._updateClock();
            this._scheduleClockTick();
            return GLib.SOURCE_REMOVE;
        });
    }

    _updateClock() {
        const now = GLib.DateTime.new_now_local();
        this._clock.text = now.format('%H:%M');
    }

    // --- visualizer (only while playing) ---
    _startVisualizer() {
        if (this._vizTimer)
            return;
        const v = this._tokens.visualizer ?? {fps: 30, minHeight: 3, maxHeight: 14};
        const interval = Math.max(33, Math.round(1000 / (v.fps ?? 30)));
        this._vizTimer = GLib.timeout_add(GLib.PRIORITY_DEFAULT, interval, () => {
            for (const bar of this._vizBars) {
                const h = (v.minHeight ?? 3) + Math.random() * ((v.maxHeight ?? 14) - (v.minHeight ?? 3));
                bar.ease({height: h, duration: interval, mode: Clutter.AnimationMode.EASE_OUT_QUAD});
            }
            return GLib.SOURCE_CONTINUE;
        });
    }

    _stopVisualizer() {
        if (this._vizTimer) {
            GLib.source_remove(this._vizTimer);
            this._vizTimer = 0;
        }
    }

    // --- mascot (resting eyes) ---
    // After `idleTimeoutMs` of no interaction (and no music), the clock gives
    // way to two eyes that blink for a while and then fall asleep. Any activity
    // wakes it and restores the clock. Controlled by the `mascot` setting
    // (off by default).
    _armIdle() {
        this._clearIdleTimer();
        if (!this._settings.get_boolean('mascot'))
            return;
        const timeout = this._tokens.mascot?.idleTimeoutMs ?? 60000;
        this._idleTimer = GLib.timeout_add(GLib.PRIORITY_DEFAULT, timeout, () => {
            this._idleTimer = 0;
            this._enterMascot();
            return GLib.SOURCE_REMOVE;
        });
    }

    _clearIdleTimer() {
        if (this._idleTimer) {
            GLib.source_remove(this._idleTimer);
            this._idleTimer = 0;
        }
    }

    // User activity: drop the mascot and re-arm the idle countdown.
    _resetIdle() {
        this._exitMascot();
        this._armIdle();
    }

    // Leaving the closed state (hover/alert): drop the mascot, don't re-arm —
    // the countdown restarts when we settle back into the closed state.
    _cancelIdle() {
        this._clearIdleTimer();
        this._exitMascot();
    }

    _enterMascot() {
        if (!this._settings.get_boolean('mascot'))
            return;
        if (this._open || this._playing || this._settings.get_boolean('minimal'))
            return;
        this._mascotActive = true;
        this._wakeEyes();
        this._updateClosedVisual();
        this._startBlink();
        // Blink calmly for a bit, then nod off.
        this._sleepTimer = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 6000, () => {
            this._sleepTimer = 0;
            this._fallAsleep();
            return GLib.SOURCE_REMOVE;
        });
    }

    _exitMascot() {
        if (!this._mascotActive && !this._blinkTimer && !this._sleepTimer)
            return;
        this._mascotActive = false;
        this._stopBlink();
        if (this._sleepTimer) {
            GLib.source_remove(this._sleepTimer);
            this._sleepTimer = 0;
        }
        this._wakeEyes();
        this._updateClosedVisual();
    }

    _wakeEyes() {
        for (const eye of this._eyeActors ?? []) {
            eye.remove_all_transitions();
            eye.set_height(this._eyeH);
            eye.opacity = 255;
        }
    }

    _startBlink() {
        this._stopBlink();
        const schedule = () => {
            const delay = 2500 + Math.floor(Math.random() * 2500);
            this._blinkTimer = GLib.timeout_add(GLib.PRIORITY_DEFAULT, delay, () => {
                this._blink();
                if (this._mascotActive)
                    schedule();
                else
                    this._blinkTimer = 0;
                return GLib.SOURCE_REMOVE;
            });
        };
        schedule();
    }

    _stopBlink() {
        if (this._blinkTimer) {
            GLib.source_remove(this._blinkTimer);
            this._blinkTimer = 0;
        }
    }

    _blink() {
        for (const eye of this._eyeActors ?? []) {
            eye.ease({
                height: 1,
                duration: 90,
                mode: Clutter.AnimationMode.EASE_IN_QUAD,
                onComplete: () =>
                    eye.ease({
                        height: this._eyeH,
                        duration: 120,
                        mode: Clutter.AnimationMode.EASE_OUT_QUAD,
                    }),
            });
        }
    }

    _fallAsleep() {
        this._stopBlink();
        const dur = this._tokens.mascot?.sleepTransitionMs ?? 1200;
        for (const eye of this._eyeActors ?? []) {
            eye.remove_all_transitions();
            eye.ease({
                height: 2,
                opacity: 110,
                duration: dur,
                mode: Clutter.AnimationMode.EASE_IN_OUT_QUAD,
            });
        }
    }

    // --- settings / teardown ---
    applySettings() {
        this._computeSizes();
        const size = this._open ? this._hoverSize : this._closed;
        this.set_size(size.w, size.h);
        this._reposition(size.w);
        this._refreshMedia();
        // Pick up a changed `mascot` toggle (exit if off, re-arm if on).
        this._resetIdle();
    }

    _onDestroy() {
        for (const t of [
            this._closeTimer,
            this._alertTimer,
            this._clockTimer,
            this._vizTimer,
            this._idleTimer,
            this._blinkTimer,
            this._sleepTimer,
        ])
            if (t)
                GLib.source_remove(t);
        this._closeTimer = this._alertTimer = this._clockTimer = this._vizTimer = 0;
        this._idleTimer = this._blinkTimer = this._sleepTimer = 0;
    }
});
