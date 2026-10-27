// The notch widget: a centered top pill that expands on hover and shows the
// music, lyrics, focus, water, shelf and to-do pages. Geometry and animation
// timings come from design tokens so it matches the Windows notch. All state is
// pushed in from the daemon via MiraClient; this file only draws and animates.
//
// The open view is built around a persistent header (status ring + date + time
// + visualizer) with left/right nav arrows flanking the page area, matching the
// Mira design mockups. The closed view is the mascot eyes (or clock / mini
// music). The eyes have a library of personality animations and a scheduled
// daily greeting, with birthday confetti.

import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import GObject from 'gi://GObject';
import St from 'gi://St';
import Pango from 'gi://Pango';
import cairo from 'gi://cairo';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PointerWatcher from 'resource:///org/gnome/shell/ui/pointerWatcher.js';
import {gazeOffset} from './gaze.js';
import {reminderState, parseWhen} from './reminders.js';
import {routineEvents, validTime} from './routines.js';
import {THEMES, EXPRESSIONS, contextMood, looksNatural, summaryText} from './mascot.js';

const PAGES = ['music', 'lyrics', 'focus', 'water', 'reminders', 'shelf', 'todo', 'summary'];

// Titles shown at the left of the header per page. Music has none (its header
// is just date/time). Two-tone titles use Pango markup.
const PAGE_TITLES = {
    music: '',
    lyrics: 'Lyrics',
    focus: 'Pomodoro',
    water: 'Water <span foreground="#3B9CFF">Reminder</span>',
    summary: 'Ziua ta',
    reminders: 'Remind<span foreground="#FFB65C">ers</span>',
    shelf: 'Shelf',
    todo: 'Todo<span foreground="#2EE67A">List</span>',
};

const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
const DAYS = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];

const ML_PER_GLASS = 250;

// --- cairo helpers ----------------------------------------------------------
function roundRect(cr, x, y, w, h, r) {
    r = Math.min(r, h / 2, w / 2);
    cr.newSubPath();
    cr.arc(x + w - r, y + r, r, -Math.PI / 2, 0);
    cr.arc(x + w - r, y + h - r, r, 0, Math.PI / 2);
    cr.arc(x + r, y + h - r, r, Math.PI / 2, Math.PI);
    cr.arc(x + r, y + r, r, Math.PI, 1.5 * Math.PI);
    cr.closePath();
}

function hex(c) {
    // "#rrggbb" or "#rrggbbaa" -> [r,g,b,a] in 0..1
    let s = c.replace('#', '');
    if (s.length === 6)
        s += 'ff';
    const n = parseInt(s, 16);
    return [((n >> 24) & 255) / 255, ((n >> 16) & 255) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255];
}

export const Notch = GObject.registerClass(
class Notch extends St.Bin {
    _init(client, tokens, settings, hooks = {}) {
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
        this._hooks = hooks;

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
        this._headerVizTimer = 0;
        this._progressTimer = 0;
        this._greetTimer = 0;
        this._artToken = 0;
        this._media = {};
        this._todoEntryVisible = false;
        this._reminders = [];
        this._reminderTimer = 0;
        this._reminderFetch = 0;
        this._reminderGeneration = 0;
        this._reminderNotified = new Set();
        this._reminderRows = [];
        this._reminderAlive = true;
        this._reminderUrgency = 'calm';
        this._reminderSubmitting = false;
        this._routineQueue = [];
        this._eyeAnimTimers = new Set();
        this._naturalPreviewTimer = 0;
        this._naturalPreviewGeneration = 0;
        this._summaryFetch = 0;
        this._bannerGeneration = 0;

        // Mascot eyes.
        this._eyeLifeTimer = 0;
        this._eyeFollowWatch = null;
        this._eyeBusy = false;
        this._greetSeqTimers = [];
        this._fxActors = [];
        this._monitorsChangedId = 0;

        this._buildContent();
        this._computeSizes();
        this.set_size(this._closed.w, this._closed.h);
        this._reposition(this._closed.w);

        this.connect('notify::hover', () => this._onHover());
        this.connect('scroll-event', (_a, ev) => this._onScroll(ev));
        this.connect('key-press-event', (_a, ev) => this._onKey(ev));
        this.connect('destroy', () => this._onDestroy());

        // Keep to the primary monitor and recentre when the layout changes.
        this._monitorsChangedId = Main.layoutManager.connect('monitors-changed', () => {
            this._computeSizes();
            const s = this._open ? this._hoverSize : this._closed;
            this.set_size(s.w, s.h);
            this._reposition(s.w);
        });

        this._startClock();
        this._refreshMedia();
        this._armIdle();
        this._scheduleGreeting();
    }

    // --- layout construction ---
    _buildContent() {
        this._stack = new St.Widget({
            layout_manager: new Clutter.BinLayout(),
            x_expand: true,
            y_expand: true,
        });
        this.set_child(this._stack);

        // Closed view: clock, or mini music (art dot + visualizer), or eyes.
        this._closedBox = new St.BoxLayout({
            style_class: 'mira-closed',
            x_align: Clutter.ActorAlign.CENTER,
            y_align: Clutter.ActorAlign.CENTER,
            x_expand: true,
            y_expand: true,
        });
        this._clock = new St.Label({style_class: 'mira-clock', y_align: Clutter.ActorAlign.CENTER});
        this._viz = this._buildVisualizer('mira-viz', 5);
        this._eyes = this._buildEyes();
        this._closedBox.add_child(this._clock);
        this._closedBox.add_child(this._viz);
        this._closedBox.add_child(this._eyes);
        this._reminderBadge = new St.Button({style_class: 'mira-reminder-badge', can_focus: true});
        this._reminderBadge.connect('clicked', () => { this.setPage(PAGES.indexOf('reminders')); this.open(); });
        this._reminderBadge.hide();
        this._closedBox.add_child(this._reminderBadge);
        this._viz.hide();
        this._eyes.hide();

        // Open view: header, body (arrows + page area), dots.
        this._openBox = new St.BoxLayout({
            vertical: true,
            style_class: 'mira-open',
            x_expand: true,
            y_expand: true,
        });
        this._buildHeader();
        const body = new St.BoxLayout({style_class: 'mira-body', y_expand: true, x_expand: true});
        this._navLeft = new St.Button({style_class: 'mira-nav', child: new St.Label({text: '‹', style_class: 'mira-nav-glyph'})});
        this._navRight = new St.Button({style_class: 'mira-nav', child: new St.Label({text: '›', style_class: 'mira-nav-glyph'})});
        this._navLeft.connect('clicked', () => this.setPage(this._page - 1));
        this._navRight.connect('clicked', () => this.setPage(this._page + 1));
        this._pageArea = new St.Widget({
            layout_manager: new Clutter.BinLayout(),
            x_expand: true,
            y_expand: true,
        });
        body.add_child(this._navLeft);
        body.add_child(this._pageArea);
        body.add_child(this._navRight);

        this._dots = new St.BoxLayout({style_class: 'mira-dots', x_align: Clutter.ActorAlign.CENTER});
        this._buildDots();

        this._openBox.add_child(this._header);
        this._openBox.add_child(body);
        this._openBox.add_child(this._dots);
        this._openBox.hide();

        // Alert view.
        this._alertBox = new St.BoxLayout({style_class: 'mira-alert', x_align: Clutter.ActorAlign.CENTER, y_align: Clutter.ActorAlign.CENTER});
        this._alertIcon = new St.Icon({style_class: 'mira-alert-icon', icon_size: 18});
        this._alertLabel = new St.Label({style_class: 'mira-alert-label', y_align: Clutter.ActorAlign.CENTER});
        this._alertBox.add_child(this._alertIcon);
        this._alertBox.add_child(this._alertLabel);
        this._alertBox.hide();

        // Greeting view: mascot eyes + a personal message.
        this._greetBox = new St.BoxLayout({style_class: 'mira-greet', x_align: Clutter.ActorAlign.CENTER, y_align: Clutter.ActorAlign.CENTER});
        this._greetEyes = this._buildEyes('mira-greet-eyes');
        this._greetLabel = new St.Label({style_class: 'mira-greet-label', y_align: Clutter.ActorAlign.CENTER});
        this._greetBox.add_child(this._greetEyes);
        const greetText = new St.BoxLayout({vertical: true, y_align: Clutter.ActorAlign.CENTER});
        greetText.add_child(this._greetLabel);
        this._routineDetail = new St.Label({style_class: 'mira-routine-detail'});
        this._routineDetail.clutter_text.single_line_mode = false;
        this._routineDetail.clutter_text.line_wrap = true;
        this._routineDetail.clutter_text.line_wrap_mode = Pango.WrapMode.WORD_CHAR;
        this._routineDetail.set_width(390);
        this._routineDetail.hide();
        greetText.add_child(this._routineDetail);
        this._greetBox.add_child(greetText);
        this._greetBox.hide();

        this._stack.add_child(this._closedBox);
        this._stack.add_child(this._openBox);
        this._stack.add_child(this._alertBox);
        this._stack.add_child(this._greetBox);

        // Overlay for transient particle effects (hearts, confetti). Free
        // positioning (no layout manager), non-reactive, drawn on top.
        this._fxLayer = new St.Widget({x_expand: true, y_expand: true, reactive: false});
        this._stack.add_child(this._fxLayer);

        this._buildMusicPage();
        this._buildLyricsPage();
        this._buildFocusPage();
        this._buildWaterPage();
        this._buildShelfPage();
        this._buildTodoPage();
        this._buildRemindersPage();
        this._buildSummaryPage();
        this.setPage(0);
    }

    _buildHeader() {
        this._header = new St.BoxLayout({style_class: 'mira-header', x_expand: true});
        this._pageTitle = new St.Label({style_class: 'mira-page-title', y_align: Clutter.ActorAlign.CENTER});
        this._pageTitle.clutter_text.use_markup = true;

        // The green "live" ring: a small day-progress arc.
        this._ring = new St.DrawingArea({style_class: 'mira-ring'});
        this._ring.set_size(16, 16);
        this._ring.connect('repaint', () => this._drawRing());

        this._date = new St.Label({style_class: 'mira-date', y_align: Clutter.ActorAlign.CENTER});
        this._time = new St.Label({style_class: 'mira-time', y_align: Clutter.ActorAlign.CENTER});
        const spacer = new St.Widget({x_expand: true});
        this._headerViz = this._buildVisualizer('mira-header-viz', 4);

        this._header.add_child(this._pageTitle);
        this._header.add_child(this._ring);
        this._header.add_child(this._date);
        this._header.add_child(this._time);
        this._header.add_child(spacer);
        this._header.add_child(this._headerViz);
    }

    _drawRing() {
        const cr = this._ring.get_context();
        const [w, h] = this._ring.get_surface_size();
        const cx = w / 2, cy = h / 2, r = Math.min(w, h) / 2 - 2;
        const now = GLib.DateTime.new_now_local();
        const frac = (now.get_hour() * 3600 + now.get_minute() * 60 + now.get_second()) / 86400;
        cr.setLineWidth(2.2);
        cr.setSourceRGBA(0.18, 0.9, 0.65, 0.25);
        cr.arc(cx, cy, r, 0, 2 * Math.PI);
        cr.stroke();
        cr.setSourceRGBA(...hex(this._tokens.colors.ring ?? '#2EE6A6'));
        cr.arc(cx, cy, r, -Math.PI / 2, -Math.PI / 2 + frac * 2 * Math.PI);
        cr.stroke();
        cr.$dispose();
    }

    _buildVisualizer(klass, n) {
        const box = new St.BoxLayout({style_class: klass, y_align: Clutter.ActorAlign.CENTER});
        box._bars = [];
        for (let i = 0; i < n; i++) {
            const bar = new St.Widget({style_class: `${klass}-bar`});
            bar.set_size(3, this._tokens.visualizer?.minHeight ?? 3);
            box._bars.push(bar);
            box.add_child(bar);
        }
        return box;
    }

    _buildEyes(klass = 'mira-eyes') {
        const box = new St.BoxLayout({
            style_class: klass,
            x_align: Clutter.ActorAlign.CENTER,
            y_align: Clutter.ActorAlign.CENTER,
        });
        box._eyes = [];
        const eyeH = klass === 'mira-greet-eyes' ? 11 : 9;
        const eyeW = eyeH;
        for (let i = 0; i < 2; i++) {
            const eye = new St.Widget({style_class: klass === 'mira-greet-eyes' ? 'mira-eye mira-eye-big' : 'mira-eye', layout_manager: new Clutter.BinLayout()});
            eye._catPupil = new St.Widget({style_class: 'mira-cat-pupil', x_align: Clutter.ActorAlign.CENTER, y_align: Clutter.ActorAlign.CENTER});
            eye.add_child(eye._catPupil);
            eye.set_pivot_point(0.5, 0.5);
            eye.set_size(eyeW, eyeH);
            eye._baseH = eyeH;
            eye._baseW = eyeW;
            box._eyes.push(eye);
            box.add_child(eye);
        }
        this._themeEyes(box._eyes, klass === 'mira-greet-eyes');
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

    // --- Music page ---
    _buildMusicPage() {
        const page = new St.BoxLayout({vertical: true, style_class: 'mira-page mira-page-music', x_expand: true, y_expand: true});

        const top = new St.BoxLayout({style_class: 'mira-music-top', x_expand: true, y_align: Clutter.ActorAlign.CENTER});
        this._art = new St.Bin({style_class: 'mira-art mira-art-empty'});
        this._artIcon = new St.Icon({icon_name: 'audio-x-generic-symbolic', icon_size: 30, style_class: 'mira-art-icon'});
        this._art.set_child(this._artIcon);
        const info = new St.BoxLayout({vertical: true, x_expand: true, y_align: Clutter.ActorAlign.CENTER, style_class: 'mira-music-info'});
        this._title = new St.Label({style_class: 'mira-title', text: '—'});
        this._artist = new St.Label({style_class: 'mira-artist'});
        this._title.clutter_text.ellipsize = 3; // END
        this._artist.clutter_text.ellipsize = 3;
        info.add_child(this._title);
        info.add_child(this._artist);
        this._btnExpand = new St.Button({style_class: 'mira-round-btn', child: new St.Label({text: '›', style_class: 'mira-round-glyph'})});
        this._btnExpand.connect('clicked', () => this.setPage(PAGES.indexOf('lyrics')));
        top.add_child(this._art);
        top.add_child(info);
        top.add_child(this._btnExpand);

        // Progress row.
        const prog = new St.BoxLayout({style_class: 'mira-prog-row', x_expand: true, y_align: Clutter.ActorAlign.CENTER});
        this._posLabel = new St.Label({style_class: 'mira-time-small', text: '0:00', y_align: Clutter.ActorAlign.CENTER});
        this._progBar = this._makeBar(this._tokens.colors.text ?? '#F2F2F3');
        this._remLabel = new St.Label({style_class: 'mira-time-small', text: '0:00', y_align: Clutter.ActorAlign.CENTER});
        prog.add_child(this._posLabel);
        prog.add_child(this._progBar.actor);
        prog.add_child(this._remLabel);

        // Controls row.
        const controls = new St.BoxLayout({style_class: 'mira-music-controls', x_expand: true, y_align: Clutter.ActorAlign.CENTER});
        this._btnLyrics = new St.Button({style_class: 'mira-ghost-btn', child: new St.Icon({icon_name: 'user-available-symbolic', icon_size: 16})});
        this._btnLyrics.connect('clicked', () => this.setPage(PAGES.indexOf('lyrics')));
        const spacerL = new St.Widget({x_expand: true});
        const spacerR = new St.Widget({x_expand: true});
        const transport = new St.BoxLayout({style_class: 'mira-controls', y_align: Clutter.ActorAlign.CENTER});
        this._btnPrev = this._iconButton('media-skip-backward-symbolic', () => this._client.previous());
        this._btnPlay = this._iconButton('media-playback-start-symbolic', () => this._client.playPause(), 'mira-ctl-btn mira-ctl-play');
        this._btnNext = this._iconButton('media-skip-forward-symbolic', () => this._client.next());
        transport.add_child(this._btnPrev);
        transport.add_child(this._btnPlay);
        transport.add_child(this._btnNext);
        controls.add_child(this._btnLyrics);
        controls.add_child(spacerL);
        controls.add_child(transport);
        controls.add_child(spacerR);

        page.add_child(top);
        page.add_child(prog);
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
            y_align: Clutter.ActorAlign.CENTER,
        });
        const search = new St.BoxLayout({style_class: 'mira-lyrics-search'});
        this._lyricsTitleInput = new St.Entry({hint_text: 'Titlu melodie', can_focus: true, x_expand: true, style_class: 'mira-reminder-entry'});
        this._lyricsArtistInput = new St.Entry({hint_text: 'Artist', can_focus: true, x_expand: true, style_class: 'mira-reminder-entry'});
        const lookup = () => this._lookupLyrics();
        this._lyricsSearchButton = this._iconButton('system-search-symbolic', lookup);
        this._lyricsSearchButton.accessible_name = 'Caută versuri după titlu și artist';
        for (const entry of [this._lyricsTitleInput, this._lyricsArtistInput]) {
            entry.clutter_text.set_max_length(200);
            entry.clutter_text.connect('activate', lookup);
        }
        search.add_child(this._lyricsTitleInput); search.add_child(this._lyricsArtistInput); search.add_child(this._lyricsSearchButton);
        this._lyricsSearchStatus = new St.Label({style_class: 'mira-lyrics-search-status', text: 'Căutare manuală pentru metadata lipsă / sesiuni private'});
        this._lyricsSearchStatus.clutter_text.ellipsize = Pango.EllipsizeMode.END;
        page.add_child(search); page.add_child(this._lyricsSearchStatus);
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
        this._plainLyrics = new St.Label({style_class: 'mira-plain-lyrics', x_expand: true});
        this._plainLyrics.clutter_text.single_line_mode = false;
        this._plainLyrics.clutter_text.ellipsize = Pango.EllipsizeMode.NONE;
        this._plainLyrics.clutter_text.line_wrap = true;
        this._plainLyricsScroll = new St.ScrollView({x_expand: true, y_expand: true, hscrollbar_policy: St.PolicyType.NEVER, vscrollbar_policy: St.PolicyType.AUTOMATIC});
        const plainBox = new St.BoxLayout({vertical: true, x_expand: true});
        plainBox.add_child(this._plainLyrics);
        this._plainLyricsScroll.set_child(plainBox);
        this._plainLyricsScroll.hide();
        page.add_child(this._plainLyricsScroll);
        this._lyricsPage = page;
        this._pageArea.add_child(page);
    }

    // --- Focus (Pomodoro) page ---
    _buildFocusPage() {
        const page = new St.BoxLayout({
            style_class: 'mira-page mira-page-focus',
            x_align: Clutter.ActorAlign.CENTER,
            y_align: Clutter.ActorAlign.CENTER,
            x_expand: true,
            y_expand: true,
        });

        // Left: big ring with the timer icon.
        this._focusRing = new St.DrawingArea({style_class: 'mira-focus-ring'});
        this._focusRing.set_size(74, 74);
        this._focusRing._frac = 0;
        this._focusRing.connect('repaint', () => this._drawFocusRing());
        const ringBin = new St.Bin({child: new St.Icon({icon_name: 'alarm-symbolic', icon_size: 26, style_class: 'mira-focus-ring-icon'}), style_class: 'mira-focus-ring-wrap'});
        const ringStack = new St.Widget({layout_manager: new Clutter.BinLayout()});
        ringStack.add_child(this._focusRing);
        ringStack.add_child(ringBin);

        // Right column: time, phase, pill controls.
        const right = new St.BoxLayout({vertical: true, style_class: 'mira-focus-right', x_align: Clutter.ActorAlign.CENTER, y_align: Clutter.ActorAlign.CENTER});
        this._focusTime = new St.Label({style_class: 'mira-focus-time', x_align: Clutter.ActorAlign.CENTER});
        this._focusPhase = new St.Label({style_class: 'mira-focus-phase', x_align: Clutter.ActorAlign.CENTER});
        const pills = new St.BoxLayout({style_class: 'mira-focus-pills', x_align: Clutter.ActorAlign.CENTER});
        const shortBreak = this._pillButton('cafe-symbolic', 'Short Break', () => this._client.pomoSkip());
        this._focusToggle = new St.Button({style_class: 'mira-pill mira-pill-accent', child: new St.Icon({icon_name: 'media-playback-start-symbolic', icon_size: 16})});
        this._focusToggle.connect('clicked', () => this._client.pomoToggle());
        const focusTime = this._pillButton('preferences-system-time-symbolic', 'Focus Time', () => this._client.pomoReset());
        pills.add_child(shortBreak);
        pills.add_child(this._focusToggle);
        pills.add_child(focusTime);
        right.add_child(this._focusTime);
        right.add_child(this._focusPhase);
        right.add_child(pills);

        page.add_child(ringStack);
        page.add_child(right);
        this._focusPage = page;
        this._pageArea.add_child(page);
    }

    _drawFocusRing() {
        const cr = this._focusRing.get_context();
        const [w, h] = this._focusRing.get_surface_size();
        const cx = w / 2, cy = h / 2, r = Math.min(w, h) / 2 - 5;
        const col = this._tokens.colors.focus ?? '#FF5A5A';
        cr.setLineWidth(5);
        cr.setSourceRGBA(...hex(col).slice(0, 3), 0.22);
        cr.arc(cx, cy, r, 0, 2 * Math.PI);
        cr.stroke();
        const frac = this._focusRing._frac ?? 0;
        cr.setSourceRGBA(...hex(col));
        cr.arc(cx, cy, r, -Math.PI / 2, -Math.PI / 2 + Math.max(0.001, frac) * 2 * Math.PI);
        cr.stroke();
        cr.$dispose();
    }

    // --- Water page ---
    _buildWaterPage() {
        const page = new St.BoxLayout({
            style_class: 'mira-page mira-page-water',
            x_expand: true,
            y_expand: true,
            y_align: Clutter.ActorAlign.CENTER,
        });

        const left = new St.BoxLayout({vertical: true, style_class: 'mira-water-left', y_align: Clutter.ActorAlign.CENTER});
        const amountRow = new St.BoxLayout({style_class: 'mira-water-amount-row', y_align: Clutter.ActorAlign.CENTER});
        this._waterAmount = new St.Label({style_class: 'mira-water-amount'});
        this._waterGoalLabel = new St.Label({style_class: 'mira-water-goal', y_align: Clutter.ActorAlign.END});
        this._waterToggle = new St.Button({style_class: 'mira-toggle on', child: new St.Icon({icon_name: 'alarm-symbolic', icon_size: 12})});
        this._waterToggle.connect('clicked', () => this._waterToggle.has_style_class_name('on') ? this._waterToggle.remove_style_class_name('on') : this._waterToggle.add_style_class_name('on'));
        amountRow.add_child(this._waterAmount);
        amountRow.add_child(this._waterGoalLabel);
        const spc = new St.Widget({x_expand: true});
        amountRow.add_child(spc);
        amountRow.add_child(this._waterToggle);

        this._waterStatus = new St.Label({style_class: 'mira-water-status'});
        this._waterStatus.clutter_text.use_markup = true;
        this._waterBar = this._makeBar(this._tokens.colors.water ?? '#3B9CFF', 6);

        const btns = new St.BoxLayout({style_class: 'mira-water-btns'});
        const drink = this._pillButton('water-drop-symbolic', 'Drink', () => {
            this._client.waterAdd();
            this._logWater(ML_PER_GLASS);
            this.refreshWater();
        }, 'mira-pill mira-pill-water');
        const undo = this._pillButton('edit-undo-symbolic', 'Undo', () => {
            this._client.waterRemove();
            this._logWater(-ML_PER_GLASS);
            this.refreshWater();
        });
        const settings = this._pillButton('emblem-system-symbolic', 'Settings', () => this._hooks.openPrefs && this._hooks.openPrefs());
        btns.add_child(drink);
        btns.add_child(undo);
        btns.add_child(settings);

        left.add_child(amountRow);
        left.add_child(this._waterStatus);
        left.add_child(this._waterBar.actor);
        left.add_child(btns);

        this._waterChart = new St.DrawingArea({style_class: 'mira-water-chart'});
        this._waterChart.connect('repaint', () => this._drawWaterChart());

        page.add_child(left);
        page.add_child(this._waterChart);
        this._waterPage = page;
        this._pageArea.add_child(page);
    }

    _drawWaterChart() {
        const cr = this._waterChart.get_context();
        const [w, h] = this._waterChart.get_surface_size();
        const data = this._waterLog ?? this._readWaterLog();
        const hours = [];
        const nowH = GLib.DateTime.new_now_local().get_hour();
        const start = Math.max(0, Math.min(nowH - 7, 16));
        for (let i = 0; i < 8; i++)
            hours.push(start + i);
        const max = Math.max(500, ...hours.map(hh => data[hh] ?? 0));
        const col = hex(this._tokens.colors.water ?? '#3B9CFF');
        const dim = hex(this._tokens.colors.waterDim ?? '#1C4E86');
        const padL = 4, padR = 30, padB = 16, padT = 6;
        const plotW = w - padL - padR, plotH = h - padB - padT;
        const bw = (plotW / hours.length) * 0.55;

        // y-axis ticks (0, max/2, max)
        cr.setSourceRGBA(0.6, 0.6, 0.64, 1);
        cr.setFontSize(9);
        const ticks = [0, Math.round(max / 2), Math.round(max)];
        for (const t of ticks) {
            const y = padT + plotH - (t / max) * plotH;
            cr.moveTo(w - padR + 4, y + 3);
            cr.showText(`${t}`);
        }
        cr.moveTo(w - padR + 20, padT + plotH + 11);
        cr.showText('ml');

        hours.forEach((hh, i) => {
            const v = data[hh] ?? 0;
            const x = padL + i * (plotW / hours.length) + (plotW / hours.length - bw) / 2;
            const bh = Math.max(2, (v / max) * plotH);
            const y = padT + plotH - bh;
            cr.setSourceRGBA(...(v > 0 ? col : dim.slice(0, 3).concat(0.5)));
            roundRect(cr, x, y, bw, bh, Math.min(3, bw / 2));
            cr.fill();
            if (hh % 3 === 0) {
                cr.setSourceRGBA(0.6, 0.6, 0.64, 1);
                cr.moveTo(x - 2, padT + plotH + 11);
                cr.showText(`${String(hh).padStart(2, '0')}`);
            }
        });
        cr.$dispose();
    }

    // --- Shelf page ---
    _buildShelfPage() {
        const page = new St.BoxLayout({
            style_class: 'mira-page mira-page-shelf',
            x_expand: true,
            y_expand: true,
            y_align: Clutter.ActorAlign.CENTER,
        });
        this._shelfScroll = new St.ScrollView({style_class: 'mira-shelf-scroll', x_expand: true, hscrollbar_policy: St.PolicyType.AUTOMATIC, vscrollbar_policy: St.PolicyType.NEVER});
        this._shelfList = new St.BoxLayout({style_class: 'mira-shelf-list', y_align: Clutter.ActorAlign.CENTER});
        this._shelfScroll.set_child(this._shelfList);
        this._shelfEmpty = new St.Label({style_class: 'mira-shelf-empty', text: 'Empty · copy a file and press Ctrl+V · or  mira-add <file>', x_align: Clutter.ActorAlign.CENTER, y_align: Clutter.ActorAlign.CENTER, x_expand: true});
        page.add_child(this._shelfScroll);
        page.add_child(this._shelfEmpty);
        this._shelfPage = page;
        this._pageArea.add_child(page);
    }

    // --- Todo page ---
    _buildTodoPage() {
        const page = new St.BoxLayout({style_class: 'mira-page mira-page-todo', x_expand: true, y_expand: true, y_align: Clutter.ActorAlign.CENTER});

        const left = new St.BoxLayout({vertical: true, style_class: 'mira-todo-left', y_align: Clutter.ActorAlign.CENTER});
        this._todoCount = new St.Label({style_class: 'mira-todo-count', text: '0/0'});
        this._todoSub = new St.Label({style_class: 'mira-todo-sub', text: 'Task Done Today'});
        const todoBtns = new St.BoxLayout({style_class: 'mira-todo-btns'});
        const add = this._pillButton('list-add-symbolic', 'Add Task', () => this._toggleTodoEntry(true));
        const edit = this._pillButton('document-edit-symbolic', 'Edit Task', () => this._toggleTodoEntry());
        todoBtns.add_child(add);
        todoBtns.add_child(edit);
        this._todoEntry = new St.Entry({style_class: 'mira-todo-entry', hint_text: 'Adaugă… / Add…', can_focus: true, x_expand: true});
        this._todoEntry.clutter_text.connect('activate', () => {
            const text = this._todoEntry.get_text().trim();
            if (text) {
                this._client.todoAdd(text);
                this._todoEntry.set_text('');
            }
        });
        this._todoEntry.hide();
        left.add_child(this._todoCount);
        left.add_child(this._todoSub);
        left.add_child(todoBtns);
        left.add_child(this._todoEntry);

        this._todoScroll = new St.ScrollView({style_class: 'mira-todo-scroll', x_expand: true, hscrollbar_policy: St.PolicyType.NEVER, vscrollbar_policy: St.PolicyType.AUTOMATIC});
        this._todoList = new St.BoxLayout({vertical: true, style_class: 'mira-todo-listbox', x_expand: true});
        this._todoScroll.set_child(this._todoList);

        page.add_child(left);
        page.add_child(this._todoScroll);
        this._todoPage = page;
        this._pageArea.add_child(page);
    }

    _toggleTodoEntry(forceShow) {
        const show = forceShow || !this._todoEntryVisible;
        this._todoEntryVisible = show;
        this._todoEntry.visible = show;
        if (show) {
            this.grab_key_focus();
            this._todoEntry.grab_key_focus();
        }
    }

    // --- Standalone reminders ---
    _buildRemindersPage() {
        const page = new St.BoxLayout({style_class: 'mira-page mira-reminders', x_expand: true, y_expand: true});
        const hero = new St.BoxLayout({vertical: true, style_class: 'mira-reminder-hero', y_align: Clutter.ActorAlign.CENTER});
        hero.add_child(new St.Label({text: 'URMEAZĂ · UP NEXT', style_class: 'mira-reminder-eyebrow'}));
        this._reminderTime = new St.Label({text: '—', style_class: 'mira-reminder-time'});
        this._reminderName = new St.Label({text: 'Niciun reminder', style_class: 'mira-reminder-name'});
        this._reminderName.clutter_text.ellipsize = Pango.EllipsizeMode.END;
        this._reminderBar = this._makeBar('#FFB65C', 3);
        hero.add_child(this._reminderTime);
        hero.add_child(this._reminderName);
        hero.add_child(this._reminderBar.actor);
        const right = new St.BoxLayout({vertical: true, style_class: 'mira-reminder-right', x_expand: true});
        const form = new St.BoxLayout({style_class: 'mira-reminder-form'});
        this._reminderTitleEntry = new St.Entry({hint_text: 'Sună-l pe Alex peste 20 de minute', style_class: 'mira-reminder-entry', can_focus: true, x_expand: true});
        this._reminderWhenEntry = new St.Entry({text: '15m', hint_text: '15m / 14:30', style_class: 'mira-reminder-when', can_focus: true});
        this._reminderTitleEntry.clutter_text.set_max_length(400);
        this._reminderTitleEntry.clutter_text.connect('text-changed', () => this._previewNatural());
        this._reminderWhenEntry.clutter_text.set_max_length(12);
        this._reminderAdd = this._iconButton('list-add-symbolic', () => this._addReminder());
        this._reminderAdd.accessible_name = 'Adaugă reminder';
        form.add_child(this._reminderTitleEntry);
        form.add_child(this._reminderWhenEntry);
        form.add_child(this._reminderAdd);
        for (const entry of [this._reminderTitleEntry, this._reminderWhenEntry])
            entry.clutter_text.connect('activate', () => this._addReminder());
        const presets = new St.BoxLayout({style_class: 'mira-reminder-presets'});
        for (const minutes of [5, 15, 30, 60])
            presets.add_child(this._pillButton(null, `${minutes} min`, () => this._reminderWhenEntry.set_text(`${minutes}m`), 'mira-reminder-preset'));
        this._reminderFeedback = new St.Label({text: '15m, 2h sau o oră exactă: 14:30', style_class: 'mira-reminder-feedback'});
        this._reminderFeedback.clutter_text.ellipsize = Pango.EllipsizeMode.END;
        const scroll = new St.ScrollView({x_expand: true, y_expand: true, hscrollbar_policy: St.PolicyType.NEVER, vscrollbar_policy: St.PolicyType.AUTOMATIC});
        this._reminderList = new St.BoxLayout({vertical: true, style_class: 'mira-reminder-list', x_expand: true});
        scroll.set_child(this._reminderList);
        right.add_child(form);
        right.add_child(presets);
        right.add_child(this._reminderFeedback);
        right.add_child(scroll);
        page.add_child(hero);
        page.add_child(right);
        this._remindersPage = page;
        this._pageArea.add_child(page);
    }

    _previewNatural() {
        if (this._naturalPreviewTimer) GLib.source_remove(this._naturalPreviewTimer);
        this._naturalPreviewTimer = 0;
        const generation = ++this._naturalPreviewGeneration;
        const input = this._reminderTitleEntry.get_text().trim();
        this._reminderWhenEntry.reactive = !looksNatural(input);
        if (!looksNatural(input)) {this._reminderFeedback.text = '15m / 14:30 sau scrie natural: «Raport mâine la 10»'; return;}
        this._naturalPreviewTimer = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 350, () => {
            this._naturalPreviewTimer = 0;
            this._client.previewNatural(input, response => {
                if (!this._reminderAlive || generation !== this._naturalPreviewGeneration) return;
                this._reminderFeedback.text = response.valid
                    ? `${response.draft.title} · ${response.draft.description}` : response.error;
            }, () => {if (this._reminderAlive && generation === this._naturalPreviewGeneration) this._reminderFeedback.text = 'Previzualizare indisponibilă · verifică daemonul';});
            return GLib.SOURCE_REMOVE;
        });
    }

    _addReminder() {
        if (this._reminderSubmitting) return;
        const title = this._reminderTitleEntry.get_text().trim();
        const natural = looksNatural(title);
        const due = parseWhen(this._reminderWhenEntry.get_text());
        if (!title || (!natural && (Array.from(title).length > 200 || !due))) {
            this._reminderFeedback.text = 'Adaugă un titlu și un timp: 15m / «Raport mâine la 10»'; return;
        }
        this._reminderSubmitting = true;
        this._reminderAdd.reactive = false;
        const ok = () => {
            if (!this._reminderAlive) return;
            this._reminderSubmitting = false; this._reminderAdd.reactive = true;
            this._reminderTitleEntry.set_text('');
            this._reminderFeedback.text = 'Salvat · Te anunț când se apropie';
            this.refreshReminders();
        };
        const fail = () => {
            if (!this._reminderAlive) return;
            this._reminderSubmitting = false; this._reminderAdd.reactive = true;
            this._reminderFeedback.text = 'Nu s-a salvat. Verifică formularea, ora și daemonul.';
        };
        if (natural) this._client.reminderAddNatural(title, ok, fail);
        else this._client.reminderAdd(title, due, ok, fail);
    }

    refreshReminders() {
        const fetch = ++this._reminderFetch;
        this._client.getReminders(items => {
            if (!this._reminderAlive || fetch !== this._reminderFetch)
                return;
            this._reminders = items.sort((a, b) => a.due - b.due || a.id - b.id);
            this._reminderList.destroy_all_children();
            this._reminderRows = [];
            for (const reminder of this._reminders) {
                const row = new St.BoxLayout({style_class: 'mira-reminder-row', x_expand: true});
                const name = new St.Label({text: `${reminder.recurrence ? '↻ ' : ''}${reminder.text}`, style_class: 'mira-reminder-row-name', x_expand: true, y_align: Clutter.ActorAlign.CENTER});
                name.clutter_text.ellipsize = Pango.EllipsizeMode.END;
                const time = new St.Label({style_class: 'mira-reminder-row-time', y_align: Clutter.ActorAlign.CENTER});
                const snooze = this._smallButton('alarm-symbolic', () => this._reminderAction('snooze', reminder.id));
                snooze.accessible_name = 'Amână cu 5 minute';
                const done = this._smallButton('object-select-symbolic', () => this._reminderAction('complete', reminder.id, reminder.due));
                done.accessible_name = 'Confirmă / șterge reminder';
                const cancel = this._smallButton('window-close-symbolic', () => this._reminderAction('remove', reminder.id));
                cancel.accessible_name = reminder.recurrence ? 'Șterge întreaga serie' : 'Șterge reminder';
                row.add_child(name); row.add_child(time); row.add_child(snooze); row.add_child(done); row.add_child(cancel);
                this._reminderList.add_child(row);
                this._reminderRows.push({reminder, row, time});
            }
            if (!items.length)
                this._reminderList.add_child(new St.Label({text: 'Un lucru mai puțin de ținut minte. ✦', style_class: 'mira-reminder-empty'}));
            if (this._reminderTimer) {
                GLib.source_remove(this._reminderTimer);
                this._reminderTimer = 0;
            }
            this._tickReminders(false);
            if (items.length)
                this._reminderTimer = GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT, 1, () => {
                    this._tickReminders(true);
                    return GLib.SOURCE_CONTINUE;
                });
            this._updateClosedVisual();
        }, () => {
            if (!this._reminderAlive || fetch !== this._reminderFetch)
                return;
            this._reminderFeedback.text = 'Remindere indisponibile · Verifică daemonul';
            if (!this._client.available) {
                this._reminders = [];
                this._reminderRows = [];
                if (this._reminderTimer)
                    GLib.source_remove(this._reminderTimer);
                this._reminderTimer = 0;
                this._tickReminders(false);
                this._updateClosedVisual();
            }
        });
    }

    _reminderAction(action, id, due) {
        const ok = () => {
            if (!this._reminderAlive)
                return;
            this._reminderNotified.delete(id);
            this._reminders = this._reminders.filter(r => r.id !== id);
            this._reminderRows = this._reminderRows.filter(r => r.reminder.id !== id);
            this._tickReminders(false);
            this.refreshReminders();
        };
        const fail = () => {
            if (this._reminderAlive)
                this._reminderFeedback.text = 'Modificarea nu s-a salvat. Încearcă din nou.';
        };
        if (action === 'snooze')
            this._client.reminderSnooze(id, ok, fail);
        else if (action === 'complete')
            this._client.reminderComplete(id, due, ok, fail);
        else
            this._client.reminderRemove(id, ok, fail);
    }

    onReminderDue(id, text) {
        if (this._reminderNotified.has(id))
            return;
        this._reminderNotified.add(id);
        Main.notify('Mira · Reminder', text);
        this._reminderFeedback.text = `Acum: ${text}`;
        // Keep the eyes and countdown visible until the user acknowledges it.
    }

    _tickReminders(animate) {
        const now = Math.floor(Date.now() / 1000);
        const next = this._reminders[0];
        const state = next ? reminderState(next, now) : null;
        this._reminderUrgency = state?.urgency ?? 'calm';
        this._reminderTime.text = state ? (state.remaining ? state.countdown : 'Acum') : '—';
        this._reminderTime.style_class = `mira-reminder-time ${this._reminderUrgency}`;
        this._reminderName.text = next?.text ?? 'Niciun reminder';
        this._reminderBar.set(state?.fraction ?? 0);
        this._reminderBadge.label = state ? `◷ ${state.remaining ? state.countdown : 'Acum'}` : '';
        this._reminderBadge.accessible_name = next ? `${next.text}, ${state.countdown}` : '';
        this._reminderBadge.style_class = `mira-reminder-badge ${this._reminderUrgency}`;
        const color = {calm: (THEMES[this._settings.get_string('mascot-theme')] ?? THEMES.classic).color, soon: '#FFB65C', urgent: '#FF7878', due: '#FF7878'}[this._reminderUrgency];
        for (const eye of this._eyes._eyes)
            eye.set_style(next ? `background-color:${color}; box-shadow:0 0 7px 0 ${color};` : '');
        for (const {reminder, row, time} of this._reminderRows) {
            const item = reminderState(reminder, now);
            time.text = item.remaining ? item.countdown : 'Acum';
            row.style_class = `mira-reminder-row ${item.urgency}`;
            if (item.urgency === 'due')
                this.onReminderDue(reminder.id, reminder.text);
        }
        if (!animate || this._open || !this._eyes.visible || !this.mapped || this._reminderUrgency === 'calm' || this._settings.get_double('motion-intensity') <= 0)
            return;
        if (this._reminderUrgency === 'soon' && now % 3 !== 0)
            return;
        const generation = ++this._reminderGeneration;
        const urgent = this._reminderUrgency === 'urgent' || this._reminderUrgency === 'due';
        this._eyes._eyes.forEach((eye, i) => {
            // Only transform the individual eyes; the parent keeps following
            // the mouse. No queued timeouts can outlive the actor.
            eye.ease({translation_x: (i ? 1 : -1) * (urgent ? 2 : 0.6) * this._settings.get_double('motion-intensity'),
                translation_y: (urgent ? -2 : -1) * this._settings.get_double('motion-intensity'), scale_y: 1 + (urgent ? 0.3 : 0.08) * this._settings.get_double('motion-intensity'),
                duration: urgent ? 110 : 240, mode: Clutter.AnimationMode.EASE_OUT_QUAD,
                onComplete: () => {
                    if (this._reminderAlive && generation === this._reminderGeneration)
                        eye.ease({translation_x: 0, translation_y: 0, scale_y: 1, duration: urgent ? 240 : 400, mode: Clutter.AnimationMode.EASE_OUT_BACK});
                }});
        });
        this._reminderBadge.ease({opacity: urgent ? 140 : 190, duration: 160,
            onComplete: () => {
                if (this._reminderAlive && generation === this._reminderGeneration)
                    this._reminderBadge.ease({opacity: 255, duration: 420});
            }});
    }

    _buildSummaryPage() {
        const page = new St.BoxLayout({vertical: true, style_class: 'mira-page mira-summary', x_expand: true});
        this._summaryStatus = new St.Label({text: 'Activitate înregistrată local · fără scoruri și presiune', style_class: 'mira-reminder-feedback'});
        page.add_child(this._summaryStatus);
        this._summaryValues = {};
        const rows = [new St.BoxLayout({style_class: 'mira-summary-row', x_expand: true}), new St.BoxLayout({style_class: 'mira-summary-row', x_expand: true})];
        [['focus_seconds','min focus'],['focus_sessions','sesiuni'],['todos_completed','sarcini'],['reminders_completed','remindere'],['water_glasses','pahare']].forEach(([key,label],index) => {
            const card = new St.BoxLayout({vertical: true, style_class: 'mira-summary-card', x_expand: true});
            const value = new St.Label({text: '—', style_class: 'mira-summary-number'});
            this._summaryValues[key] = value;
            card.add_child(value); card.add_child(new St.Label({text: label, style_class: 'mira-reminder-feedback'}));
            rows[index < 3 ? 0 : 1].add_child(card);
        });
        rows.forEach(row => page.add_child(row));
        page.add_child(this._pillButton('view-refresh-symbolic','Actualizează',()=>this.refreshSummary()));
        this._summaryPage = page; this._pageArea.add_child(page);
    }
    refreshSummary() {
        const fetch = ++this._summaryFetch;
        this._client.getSummary(stats => {
            if (!this._reminderAlive || fetch !== this._summaryFetch) return;
            this._summaryStatus.text = `${stats.day} · Doar activitatea înregistrată de Mira`;
            for (const [key,label] of Object.entries(this._summaryValues))
                label.text = String(key === 'focus_seconds' ? Math.floor(stats[key] / 60) : stats[key]);
        }, () => {if (this._reminderAlive && fetch === this._summaryFetch) this._summaryStatus.text = 'Rezumat indisponibil · verifică daemonul';});
    }

    _themeEyes(eyes, big = false) {
        const key = this._settings.get_string('mascot-theme');
        const theme = THEMES[key] ?? THEMES.classic;
        for (const eye of eyes) {
            for (const name of Object.keys(THEMES)) eye.remove_style_class_name(`theme-${name}`);
            eye.add_style_class_name(`theme-${THEMES[key] ? key : 'classic'}`);
            eye._baseW = theme.width + (big ? 2 : 0); eye._baseH = theme.height + (big ? 2 : 0);
            eye.set_size(eye._baseW, eye._baseH);
            eye._catPupil.visible = key === 'cat';
            eye.clip_to_allocation = key === 'cat';
        }
    }

    previewMascot(name) {
        if (['urgent','due'].includes(this._reminderUrgency)) { Main.notify('Studio mascotă', 'Confirmă sau amână reminderul apropiat înainte de previzualizare.'); return; }
        const contexts = ['music','focus','evening','reminder'];
        if (!EXPRESSIONS.includes(name) && !contexts.includes(name)) return;
        this.showGreeting(false);
        this._clearGreetSeq();
        this._wakeEyeActors(this._greetEyes._eyes);
        this._greetLabel.text = `Studio · ${(THEMES[this._settings.get_string('mascot-theme')] ?? THEMES.classic).label}`;
        this._routineDetail.text = `${name} · intensitate ${Math.round(this._settings.get_double('motion-intensity') * 100)}%`;
        this._routineDetail.show();
        if (contexts.includes(name)) this._contextEyes(this._greetEyes._eyes, name);
        else this._eyeAnim(this._greetEyes._eyes, name);
    }
    _contextEyes(eyes, mood) {
        const strength = this._settings.get_double('motion-intensity');
        if (strength <= 0) return;
        eyes.forEach((eye,index) => {
            const target = mood === 'music' ? {translation_y: (index ? 2 : -2) * strength, scale_y: 1 + 0.15 * strength}
                : mood === 'focus' ? {height: eye._baseH * (1 - 0.22 * strength)}
                : mood === 'evening' ? {height: eye._baseH * (1 - 0.45 * strength)}
                : {translation_x: (index ? 2 : -2) * strength, scale_y: 1 + 0.25 * strength};
            eye.ease({...target, duration: mood === 'music' ? 180 : 400, mode: Clutter.AnimationMode.EASE_OUT_QUAD,
                onComplete: () => {if (this._reminderAlive) eye.ease({translation_x: 0, translation_y: 0, height: eye._baseH, scale_y: 1, duration: 650, mode: Clutter.AnimationMode.EASE_OUT_QUAD});}});
        });
    }

    // --- reusable widgets ---
    _iconButton(icon, onClick, klass = 'mira-ctl-btn') {
        const btn = new St.Button({style_class: klass, can_focus: true});
        btn.set_child(new St.Icon({icon_name: icon, icon_size: 16}));
        btn.connect('clicked', onClick);
        return btn;
    }

    _smallButton(icon, onClick) {
        const btn = new St.Button({style_class: 'mira-ctl-btn', can_focus: true});
        btn.set_child(new St.Icon({icon_name: icon, icon_size: 12}));
        btn.connect('clicked', onClick);
        return btn;
    }

    _pillButton(icon, text, onClick, klass = 'mira-pill') {
        const btn = new St.Button({style_class: klass, can_focus: true});
        const box = new St.BoxLayout({style_class: 'mira-pill-box', y_align: Clutter.ActorAlign.CENTER});
        if (icon)
            box.add_child(new St.Icon({icon_name: icon, icon_size: 13}));
        box.add_child(new St.Label({text, y_align: Clutter.ActorAlign.CENTER}));
        btn.set_child(box);
        btn.connect('clicked', onClick);
        return btn;
    }

    // A rounded progress bar backed by a DrawingArea. Returns {actor, set(frac)}.
    _makeBar(accent, thickness = 4) {
        const area = new St.DrawingArea({style_class: 'mira-bar', x_expand: true});
        area.set_height(thickness + 2);
        area._frac = 0;
        area._accent = accent;
        area._thick = thickness;
        area.connect('repaint', () => {
            const cr = area.get_context();
            const [w, h] = area.get_surface_size();
            const t = area._thick;
            const y = (h - t) / 2;
            cr.setSourceRGBA(1, 1, 1, 0.16);
            roundRect(cr, 0, y, w, t, t / 2);
            cr.fill();
            const fw = Math.max(0, Math.min(1, area._frac)) * w;
            if (fw > 0) {
                cr.setSourceRGBA(...hex(area._accent));
                roundRect(cr, 0, y, fw, t, t / 2);
                cr.fill();
            }
            cr.$dispose();
        });
        return {
            actor: area,
            set(frac) {
                area._frac = frac;
                area.queue_repaint();
            },
        };
    }

    _fmtTime(secs) {
        secs = Math.max(0, secs | 0);
        const m = Math.floor(secs / 60);
        const s = secs % 60;
        return `${m}:${String(s).padStart(2, '0')}`;
    }

    // --- page refreshers (called on navigation and on daemon events) ---
    refreshPomodoro() {
        if (!this._focusPhase)
            return;
        const p = this._client.pomodoro();
        const names = {work: 'Focus time', short_break: 'Short break', long_break: 'Long break'};
        this._focusPhase.text = names[p.phase] ?? 'Focus time';
        this._focusTime.text = this._fmtTime(p.remaining);
        const total = {work: 25 * 60, short_break: 5 * 60, long_break: 15 * 60}[p.phase] ?? 25 * 60;
        this._focusRing._frac = 1 - p.remaining / total;
        this._focusRing.queue_repaint();
        this._focusToggle.get_child().icon_name = p.running
            ? 'media-playback-pause-symbolic'
            : 'media-playback-start-symbolic';
    }

    refreshWater() {
        if (!this._waterAmount)
            return;
        const w = this._client.water();
        const liters = (w.glasses * ML_PER_GLASS) / 1000;
        const goalL = (w.goal * ML_PER_GLASS) / 1000;
        const pct = goalL > 0 ? Math.min(100, Math.round((liters / goalL) * 100)) : 0;
        this._waterAmount.text = `${liters.toFixed(1)}`;
        this._waterGoalLabel.text = ` / ${goalL.toFixed(1)} L`;
        const reached = pct >= 100;
        this._waterStatus.clutter_text.set_markup(
            `Today · <span foreground="#3B9CFF" weight="bold">${pct}%</span> · ${reached ? 'Goal reached' : 'Keep going'}`
        );
        this._waterBar.set(pct / 100);
        if (this._waterChart.visible)
            this._waterChart.queue_repaint();
    }

    refreshTodo() {
        if (!this._todoList)
            return;
        this._client.getTodos(items => {
            this._todoList.destroy_all_children();
            const done = items.filter(i => i[2]).length;
            this._todoCount.text = `${done}/${items.length}`;
            for (const [id, text, isDone, remind] of items.slice(0, 12)) {
                const row = new St.BoxLayout({style_class: isDone ? 'mira-trow done' : 'mira-trow', x_expand: true});
                const check = new St.Button({style_class: isDone ? 'mira-tcheck on' : 'mira-tcheck'});
                check.set_child(new St.Icon({icon_name: isDone ? 'emblem-ok-symbolic' : 'checkbox-symbolic', icon_size: 13}));
                check.connect('clicked', () => this._client.todoToggle(id));
                const label = new St.Label({text, x_expand: true, y_align: Clutter.ActorAlign.CENTER, style_class: isDone ? 'mira-ttext done' : 'mira-ttext'});
                label.clutter_text.ellipsize = 3;
                const del = this._smallButton('window-close-symbolic', () => this._client.todoRemove(id));
                const bell = this._smallButton('alarm-symbolic', () => this._client.todoSetReminder(id, Math.floor(Date.now() / 1000) + 3600));
                if (remind > 0)
                    bell.add_style_class_name('active');
                row.add_child(check);
                row.add_child(label);
                row.add_child(bell);
                row.add_child(del);
                this._todoList.add_child(row);
            }
        });
    }

    refreshShelf() {
        if (!this._shelfList)
            return;
        this._client.getShelf(items => {
            this._shelfList.destroy_all_children();
            const list = items.slice(0, 12);
            this._shelfEmpty.visible = list.length === 0;
            this._shelfScroll.visible = list.length > 0;
            for (const [id, path] of list) {
                const card = new St.BoxLayout({vertical: true, style_class: 'mira-shelf-card', x_align: Clutter.ActorAlign.CENTER});
                const thumb = new St.Bin({style_class: 'mira-shelf-thumb'});
                const gicon = this._fileIcon(path);
                thumb.set_child(new St.Icon({gicon, icon_size: 34, style_class: 'mira-shelf-thumb-icon'}));
                const name = new St.Label({text: this._basename(path), style_class: 'mira-shelf-cname'});
                name.clutter_text.ellipsize = 3;
                const size = new St.Label({text: this._fileSize(path), style_class: 'mira-shelf-csize'});
                const del = new St.Button({style_class: 'mira-shelf-del', child: new St.Icon({icon_name: 'window-close-symbolic', icon_size: 11})});
                del.connect('clicked', () => this._client.shelfRemove(id));
                const thumbWrap = new St.Widget({layout_manager: new Clutter.BinLayout()});
                thumbWrap.add_child(thumb);
                del.set_x_align(Clutter.ActorAlign.END);
                del.set_y_align(Clutter.ActorAlign.START);
                thumbWrap.add_child(del);
                card.add_child(thumbWrap);
                card.add_child(name);
                card.add_child(size);
                this._shelfList.add_child(card);
            }
        });
    }

    _basename(path) {
        return path.split('/').pop();
    }

    _fileIcon(path) {
        try {
            const f = Gio.File.new_for_path(path);
            const info = f.query_info('standard::icon,thumbnail::path', Gio.FileQueryInfoFlags.NONE, null);
            const thumb = info.get_attribute_byte_string('thumbnail::path');
            if (thumb)
                return Gio.FileIcon.new(Gio.File.new_for_path(thumb));
            return info.get_icon();
        } catch (e) {
            return Gio.ThemedIcon.new('text-x-generic-symbolic');
        }
    }

    _fileSize(path) {
        try {
            const info = Gio.File.new_for_path(path).query_info('standard::size', Gio.FileQueryInfoFlags.NONE, null);
            let b = info.get_size();
            const units = ['B', 'KB', 'MB', 'GB'];
            let u = 0;
            while (b >= 1024 && u < units.length - 1) {
                b /= 1024;
                u++;
            }
            return `${b < 10 && u > 0 ? b.toFixed(1) : Math.round(b)} ${units[u]}`;
        } catch (e) {
            return '';
        }
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
        const g = t.greeting ?? t.alert;
        this._greetSize = {w: g.width * scale, h: g.height, r: g.radiusBottom};
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
        if (this.hover) {
            if (this._reminders.length && this._reminderUrgency !== 'calm')
                this.setPage(PAGES.indexOf('reminders'));
            this.open();
        } else
            this._scheduleClose();
    }

    open() {
        this._bannerGeneration++;
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
        this._greetBox.hide();
        this._openBox.show();
        this.grab_key_focus();
        this._animateTo(this._hoverSize, this._tokens.motion.open.durationMs, Clutter.AnimationMode.EASE_OUT_BACK);
        this._updateHeader();
        this._startHeaderViz();
        this._startProgress();
        this._refreshPage(PAGES[this._page]);
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
        this._stopHeaderViz();
        this._stopProgress();
        this._animateTo(this._closed, this._tokens.motion.close.durationMs, Clutter.AnimationMode.EASE_OUT_QUAD, () => {
            if (!this._open) {
                this._openBox.hide();
                this._closedBox.show();
                this._updateClosedVisual();
                this._armIdle();
                this._drainRoutines();
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
        this._updateHeader();
        this._refreshPage(PAGES[this._page]);
    }

    _showPageActor(i) {
        const name = PAGES[i];
        this._musicPage.visible = name === 'music';
        this._lyricsPage.visible = name === 'lyrics';
        this._focusPage.visible = name === 'focus';
        this._waterPage.visible = name === 'water';
        this._shelfPage.visible = name === 'shelf';
        this._todoPage.visible = name === 'todo';
        this._remindersPage.visible = name === 'reminders';
        this._summaryPage.visible = name === 'summary';
    }

    _updateHeader() {
        if (!this._header)
            return;
        const name = PAGES[this._page];
        const title = PAGE_TITLES[name] ?? '';
        this._pageTitle.visible = title !== '';
        if (title)
            this._pageTitle.clutter_text.set_markup(title);
        this._updateClock();
        this._ring.queue_repaint();
    }

    _refreshPage(name) {
        switch (name) {
        case 'lyrics':
            this._refreshLyrics();
            break;
        case 'focus':
            this.refreshPomodoro();
            break;
        case 'water':
            this.refreshWater();
            this._waterChart.queue_repaint();
            break;
        case 'summary':
            this.refreshSummary();
            break;
        case 'reminders':
            this.refreshReminders();
            break;
        case 'todo':
            this.refreshTodo();
            break;
        case 'shelf':
            this.refreshShelf();
            break;
        case 'music':
            this._refreshMedia();
            break;
        }
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
        const focus = global.stage.get_key_focus();
        if (focus && [this._reminderTitleEntry, this._reminderWhenEntry, this._todoEntry, this._lyricsTitleInput, this._lyricsArtistInput].some(entry => entry?.contains(focus)))
            return Clutter.EVENT_PROPAGATE;
        const sym = ev.get_key_symbol();
        const state = ev.get_state();
        const ctrl = (state & Clutter.ModifierType.CONTROL_MASK) !== 0;
        // Ctrl+V on the shelf page pastes copied files/paths into the shelf —
        // the practical stand-in for drag-and-drop, which Wayland does not allow
        // onto shell chrome (see also `mira-add`).
        if (ctrl && (sym === Clutter.KEY_v || sym === Clutter.KEY_V) && PAGES[this._page] === 'shelf') {
            this._pasteToShelf();
            return Clutter.EVENT_STOP;
        }
        if (sym === Clutter.KEY_Left)
            this.setPage(this._page - 1);
        else if (sym === Clutter.KEY_Right)
            this.setPage(this._page + 1);
        else
            return Clutter.EVENT_PROPAGATE;
        return Clutter.EVENT_STOP;
    }

    // Read the clipboard and add any file paths / file:// URIs to the shelf.
    _pasteToShelf() {
        const cb = St.Clipboard.get_default();
        cb.get_text(St.ClipboardType.CLIPBOARD, (_c, text) => {
            if (!text)
                return;
            let added = 0;
            for (let line of text.split(/[\r\n]+/)) {
                line = line.trim();
                if (line === '' || line === 'copy' || line === 'cut')
                    continue;
                let path = line;
                if (path.startsWith('file://')) {
                    try {
                        path = GLib.filename_from_uri(path)[0];
                    } catch (e) {
                        continue;
                    }
                }
                if (path.startsWith('/') && GLib.file_test(path, GLib.FileTest.EXISTS)) {
                    this._client.shelfAdd(path);
                    added++;
                }
            }
            if (added > 0)
                this.refreshShelf();
        });
    }

    // --- media / lyrics / power updates ---
    _refreshMedia() {
        const m = this._client.media();
        this._media = m;
        const playing = m.status === 'Playing';
        const hasTrack = m.title !== '' || m.artist !== '';

        if (this._title) {
            const hidden = /^(a site is playing media|media is playing|un site redă (conținut )?media)$/i.test(m.title ?? '');
            this._title.text = hidden ? 'Sesiune privată · titlu ascuns' : (m.title || 'Nicio melodie detectată');
            this._artist.text = hidden ? 'Introdu titlul și artistul în pagina Lyrics' : (m.artist || '');
            this._btnPlay.get_child().icon_name = playing
                ? 'media-playback-pause-symbolic'
                : 'media-playback-start-symbolic';
            this._btnNext.reactive = m.canGoNext;
            this._btnPrev.reactive = m.canGoPrevious;
            this._setArt(m.artUrl || '');
            this._progBase = {pos: m.positionMicros, len: m.lengthMicros, at: Date.now(), playing};
            this._updateProgress();
        }

        this._hasTrack = hasTrack;
        this._playing = playing;
        this._updateClosedVisual();
        if (playing)
            this._startVisualizer();
        else
            this._stopVisualizer();
        if (this._open && PAGES[this._page] === 'lyrics')
            this._renderLyrics();
    }

    _setArt(url) {
        if (!this._art)
            return;
        this._artToken++;
        if (!url) {
            this._art.set_style('');
            this._art.add_style_class_name('mira-art-empty');
            this._artIcon.show();
            return;
        }
        if (url.startsWith('file://')) {
            this._applyArtFile(decodeURIComponent(url.slice(7)));
        } else if (url.startsWith('/')) {
            this._applyArtFile(url);
        } else if (url.startsWith('http')) {
            this._downloadArt(url, this._artToken);
        } else {
            this._art.add_style_class_name('mira-art-empty');
            this._artIcon.show();
        }
    }

    _applyArtFile(path) {
        if (!GLib.file_test(path, GLib.FileTest.EXISTS)) {
            this._art.add_style_class_name('mira-art-empty');
            this._artIcon.show();
            return;
        }
        this._art.remove_style_class_name('mira-art-empty');
        this._artIcon.hide();
        const uri = GLib.filename_to_uri(path, null);
        this._art.set_style(`background-image:url("${uri}");background-size:cover;background-position:center;`);
    }

    _downloadArt(url, token) {
        const cacheDir = GLib.build_filenamev([GLib.get_user_cache_dir(), 'mira', 'art']);
        GLib.mkdir_with_parents(cacheDir, 0o755);
        let h = 0;
        for (let i = 0; i < url.length; i++)
            h = (h * 31 + url.charCodeAt(i)) >>> 0;
        const dest = GLib.build_filenamev([cacheDir, `${h}.img`]);
        if (GLib.file_test(dest, GLib.FileTest.EXISTS)) {
            if (token === this._artToken)
                this._applyArtFile(dest);
            return;
        }
        try {
            const src = Gio.File.new_for_uri(url);
            src.load_contents_async(null, (file, res) => {
                try {
                    const [ok, contents] = file.load_contents_finish(res);
                    if (ok && token === this._artToken) {
                        GLib.file_set_contents(dest, contents);
                        this._applyArtFile(dest);
                    }
                } catch (e) {
                    if (token === this._artToken) {
                        this._art.add_style_class_name('mira-art-empty');
                        this._artIcon.show();
                    }
                }
            });
        } catch (e) {
            this._art.add_style_class_name('mira-art-empty');
            this._artIcon.show();
        }
    }

    // Music progress ticker.
    _startProgress() {
        if (this._progressTimer)
            return;
        this._progressTimer = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 500, () => {
            if (PAGES[this._page] === 'music' && this._open)
                this._updateProgress();
            return GLib.SOURCE_CONTINUE;
        });
    }

    _stopProgress() {
        if (this._progressTimer) {
            GLib.source_remove(this._progressTimer);
            this._progressTimer = 0;
        }
    }

    _updateProgress() {
        const b = this._progBase;
        if (!b || !this._progBar)
            return;
        let cur = b.pos;
        if (b.playing)
            cur += (Date.now() - b.at) * 1000;
        if (b.len > 0)
            cur = Math.min(cur, b.len);
        const posS = cur / 1e6;
        const lenS = b.len / 1e6;
        this._posLabel.text = this._fmtTime(posS);
        this._remLabel.text = b.len > 0 ? `-${this._fmtTime(lenS - posS)}` : this._fmtTime(posS);
        this._progBar.set(b.len > 0 ? cur / b.len : 0);
    }

    _updateClosedVisual() {
        const minimal = this._settings.get_boolean('minimal');
        const pending = this._reminders.length > 0;
        const showEyes = (this._settings.get_boolean('mascot') || pending) && !minimal;
        const showViz = !showEyes && this._playing && !minimal;
        this._eyes.visible = showEyes;
        this._viz.visible = showViz;
        this._clock.visible = !showEyes && !showViz;
        this._reminderBadge.visible = pending && !minimal;

        if (showEyes && !this._open)
            this._startEyeLife();
        else
            this._stopEyeLife();
    }

    refreshMedia() {
        this._refreshMedia();
    }

    refreshPower() {}

    _lookupLyrics() {
        if (this._lyricsLookingUp)
            return;
        const title = this._lyricsTitleInput.get_text().trim();
        const artist = this._lyricsArtistInput.get_text().trim();
        if (!title) { this._lyricsSearchStatus.text = 'Introdu titlul melodiei'; return; }
        this._lyricsLookingUp = true;
        this._lyricsSearchButton.reactive = false;
        this._lyricsSearchStatus.text = 'Caut versurile pe LRCLIB…';
        const complete = text => {
            if (!this._reminderAlive)
                return;
            this._lyricsLookingUp = false;
            this._lyricsSearchButton.reactive = true;
            this._lyricsSearchStatus.text = text;
            this._refreshLyrics();
        };
        this._client.lookupLyrics(title, artist, found => complete(found ? `Manual: ${artist ? artist + ' — ' : ''}${title}` : 'Nu am găsit versuri. Verifică titlul și artistul.'),
            () => complete('Căutarea a eșuat. Verifică internetul și daemonul.'));
    }

    _refreshLyrics() {
        if (PAGES[this._page] !== 'lyrics' && !this._open)
            return;
        const fetch = this._lyricsFetch = (this._lyricsFetch ?? 0) + 1;
        this._client.getLines(lines => {
            if (!this._reminderAlive || fetch !== this._lyricsFetch)
                return;
            this._lines = lines;
            this._renderLyrics();
        });
    }

    onLyricsChanged() {
        if (!this._lyricsLookingUp && this._lyricsSearchStatus) {
            const meta = this._client.lyricsMeta();
            this._lyricsSearchStatus.text = meta.manualQuery
                ? `${meta.available ? 'Manual' : 'Fără rezultate'}: ${meta.manualQuery}`
                : 'Căutare manuală pentru metadata lipsă / sesiuni private';
        }
        this._activeLine = -1;
        this._refreshLyrics();
    }

    onActiveLine(index) {
        this._activeLine = index;
        this._renderLyrics();
    }

    _renderLyrics() {
        if (!this._lyricLabels)
            return;
        const plain = this._lines.length > 0 && this._client.lyricsMeta?.().synced === false;
        this._plainLyricsScroll.visible = plain;
        this._lyricLabels.forEach(btn => { btn.visible = !plain; });
        if (plain) {
            this._plainLyrics.text = this._lines.map(line => line[1]).join('\n');
            return;
        }
        const cur = Math.max(0, this._activeLine);
        const rows = [cur - 1, cur, cur + 1];
        const placeholder = this._lyricsPlaceholder();
        this._lyricLabels.forEach((btn, i) => {
            const idx = rows[i];
            const line = idx >= 0 && idx < this._lines.length ? this._lines[idx] : null;
            btn._lineIndex = idx;
            btn._label.text = line ? line[1] : (i === 1 && this._lines.length === 0 ? placeholder : '');
            if (i === 1)
                btn.add_style_class_name('active');
            else
                btn.remove_style_class_name('active');
            btn.reactive = !!line;
        });
    }

    // Middle-line text when no lyrics are loaded, tailored to the media state.
    _lyricsPlaceholder() {
        const m = this._media || {};
        const hasTrack = (m.title && m.title !== '') || (m.artist && m.artist !== '');
        const meta = this._client.lyricsMeta ? this._client.lyricsMeta() : {};
        if (/^(a site is playing media|media is playing|un site redă (conținut )?media)$/i.test(m.title ?? ''))
            return 'Titlu ascuns de browser · caută manual mai sus';
        if (!hasTrack)
            return '♪  nothing playing';
        if (meta && meta.available === false)
            return '♪  no lyrics for this track';
        return '♪  searching lyrics…';
    }

    _onLyricClicked(btn) {
        if (typeof btn._lineIndex === 'number' && btn._lineIndex >= 0)
            this._client.seekToLine(btn._lineIndex);
    }

    // --- alerts ---
    showAlert(type, arg) {
        this._bannerGeneration++;
        this._open = false;
        this._stopHeaderViz();
        this._stopProgress();
        this._clearGreetSeq();
        const map = {
            'charger-connected': ['battery-full-charging-symbolic', `⚡ ${Math.round(arg)}%`],
            'charger-disconnected': ['battery-symbolic', `🔋 ${Math.round(arg)}%`],
            'reminder': ['alarm-symbolic', `⏰ ${arg}`],
            'pomodoro-phase': ['appointment-soon-symbolic', arg === 'work' ? 'Focus!' : 'Pauză · Break'],
        };
        const [icon, text] = map[type] ?? ['dialog-information-symbolic', ''];
        this._alertIcon.icon_name = icon;
        this._alertLabel.text = text;

        this._cancelIdle();
        this._closedBox.hide();
        this._openBox.hide();
        this._greetBox.hide();
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
                this._drainRoutines();
            } else {
                this.open();
            }
            return GLib.SOURCE_REMOVE;
        });
    }

    // Daily routines use wall-clock deadlines and persisted delivery markers.
    _tickRoutines() {
        if (!this._routineQueue || !this._greetLabel) return;
        const s = this._settings;
        const events = routineEvents({lunchEnabled: s.get_boolean('lunch-enabled'), lunchTime: s.get_string('lunch-time'),
            workEndEnabled: s.get_boolean('work-end-enabled'), workEndTime: s.get_string('work-end-time'),
            summaryEnabled: s.get_boolean('daily-summary-enabled'), summaryTime: s.get_string('daily-summary-time'),
            leadMinutes: s.get_int('work-end-lead-minutes'), weekdaysOnly: s.get_boolean('routine-weekdays-only')}, new Date(), s.get_value('routine-delivered').deepUnpack());
        if (events.length) {
            const delivered = s.get_value('routine-delivered').deepUnpack();
            for (const event of events) {
                delivered[event.type] = event.key;
                this._routineQueue.push(event.type);
                const [title, message] = this._routineMessage(event.type);
                Main.notify(title, message);
            }
            s.set_value('routine-delivered', new GLib.Variant('a{ss}', delivered));
        }
        this._drainRoutines();
    }

    _routineMessage(type) {
        const name = this._settings.get_string('greeting-name').trim();
        const suffix = name ? `, ${name}` : '';
        return {
            lunch: ['Poftă bună! 🍽', `E timpul pentru pauza de prânz${suffix}. Ai grijă de tine!`],
            'work-soon': ['Mai e puțin ✨', `Programul se termină la ${this._settings.get_string('work-end-time')}. Încheie în ritmul tău${suffix}.`],
            summary: ['Ziua ta, în câteva cifre ✨', 'Activitatea ta rămâne locală. Fiecare zi are ritmul ei.'],
            'work-end': ['Pe mâine! 👋', `Ai încheiat programul${suffix}. Mulțumesc pentru azi — bucură-te de timpul tău!`],
        }[type] ?? ['Mira', ''];
    }

    _drainRoutines() {
        if (this._routineQueue.length && !this._open && !this._alertTimer && this.mapped)
            this.showRoutine(this._routineQueue.shift());
    }

    showRoutine(type) {
        if (!['lunch', 'work-soon', 'work-end', 'summary'].includes(type))
            return;
        // Reuse the greeting lifecycle so every timeout and transition is
        // cancelled on replacement or extension disable.
        this.showGreeting(false);
        const [title, message] = this._routineMessage(type);
        this._greetLabel.text = title;
        this._routineDetail.text = message;
        this._routineDetail.show();
        const banner = this._bannerGeneration;
        if (type === 'summary' || (type === 'work-end' && this._settings.get_boolean('daily-summary-enabled'))) {
            this._client.getSummary(stats => {
                if (!this._reminderAlive || banner !== this._bannerGeneration || !this._greetBox.visible) return;
                this._routineDetail.text = summaryText(stats);
            });
        }
        this._greetLabel.style_class = 'mira-greet-label mira-routine-label';
        this._greetLabel.clutter_text.single_line_mode = false;
        this._greetLabel.clutter_text.ellipsize = Pango.EllipsizeMode.NONE;
        this._greetLabel.clutter_text.line_wrap = true;
        this._animateTo({w: Math.max(this._greetSize.w, 540), h: 90, r: this._greetSize.r}, 420, Clutter.AnimationMode.EASE_OUT_BACK);
        this._clearGreetSeq();
        this._wakeEyeActors(this._greetEyes._eyes);
        this._eyeAnim(this._greetEyes._eyes, type === 'work-soon' ? 'surprised' : 'squintHappy');
        this._greetSeqTimers.push(GLib.timeout_add(GLib.PRIORITY_DEFAULT, 700, () => {
            this._eyeAnim(this._greetEyes._eyes, type === 'lunch' ? 'hearts' : 'wink');
            if (type === 'work-end') this._spawnParticles('confetti', 22);
            return GLib.SOURCE_REMOVE;
        }));
        this._greetSeqTimers.push(GLib.timeout_add(GLib.PRIORITY_DEFAULT, 1800, () => {
            this._eyeAnim(this._greetEyes._eyes, 'squintHappy');
            return GLib.SOURCE_REMOVE;
        }));
    }

    // --- greeting ---
    _scheduleGreeting() {
        if (this._greetTimer) {
            GLib.source_remove(this._greetTimer);
            this._greetTimer = 0;
        }
        if (!this._settings.get_boolean('greeting-enabled'))
            return;
        const spec = this._settings.get_string('greeting-time') || '08:20';
        const [hh, mm] = (validTime(spec) ?? '08:20').split(':').map(Number);
        const now = GLib.DateTime.new_now_local();
        let target = GLib.DateTime.new_local(now.get_year(), now.get_month(), now.get_day_of_month(), hh, mm, 0);
        if (target.difference(now) <= 0)
            target = target.add_days(1);
        const secs = Math.max(1, Math.round(target.difference(now) / 1e6));
        this._greetTimer = GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT, secs, () => {
            this._greetTimer = 0;
            this.showGreeting(this._isBirthdayToday());
            this._scheduleGreeting();
            return GLib.SOURCE_REMOVE;
        });
    }

    _isBirthdayToday() {
        const spec = (this._settings.get_string('greeting-birthday') || '').trim();
        const m = spec.match(/^(\d{1,2})-(\d{1,2})$/);
        if (!m)
            return false;
        const now = GLib.DateTime.new_now_local();
        return now.get_month() === parseInt(m[1], 10) && now.get_day_of_month() === parseInt(m[2], 10);
    }

    showGreeting(birthday = false) {
        this._bannerGeneration++;
        this._routineDetail.hide();
        this._open = false;
        this._stopHeaderViz();
        this._stopProgress();
        this._greetLabel.style_class = 'mira-greet-label';
        this._greetLabel.clutter_text.single_line_mode = true;
        this._greetLabel.clutter_text.line_wrap = false;
        this._greetLabel.clutter_text.ellipsize = Pango.EllipsizeMode.END;
        // Name is optional; with none set the greeting simply omits it.
        const name = (this._settings.get_string('greeting-name') || '').trim();
        const suffix = name ? `, ${name}` : '';
        if (birthday) {
            this._greetLabel.text = `La mulți ani${suffix}! 🎉`;
        } else {
            const spec = (this._settings.get_string('greeting-time') || '08:20').split(':');
            const hour = parseInt(spec[0], 10) || 8;
            const part = hour < 12 ? 'Bună dimineața' : hour < 18 ? 'Bună ziua' : 'Bună seara';
            this._greetLabel.text = `${part}${suffix}`;
        }

        if (this._alertTimer) {
            GLib.source_remove(this._alertTimer);
            this._alertTimer = 0;
        }
        this._cancelIdle();
        this._closedBox.hide();
        this._openBox.hide();
        this._alertBox.hide();
        this._greetBox.show();
        // Birthday uses a wider banner so the confetti has room.
        const size = birthday ? {w: Math.max(this._greetSize.w, 520), h: this._greetSize.h, r: this._greetSize.r} : this._greetSize;
        this._animateTo(size, this._tokens.motion.alertIn.durationMs, Clutter.AnimationMode.EASE_OUT_BACK);

        this._clearGreetSeq();
        this._wakeEyeActors(this._greetEyes._eyes);

        if (birthday) {
            // Confetti bursts + excited, loving eyes.
            this._eyeAnim(this._greetEyes._eyes, 'surprised');
            this._spawnParticles('confetti', 48);
            this._greetSeqTimers.push(GLib.timeout_add(GLib.PRIORITY_DEFAULT, 900, () => {
                this._eyeAnim(this._greetEyes._eyes, 'hearts');
                return GLib.SOURCE_REMOVE;
            }));
            this._greetSeqTimers.push(GLib.timeout_add(GLib.PRIORITY_DEFAULT, 2000, () => {
                this._spawnParticles('confetti', 36);
                this._eyeAnim(this._greetEyes._eyes, 'excited');
                return GLib.SOURCE_REMOVE;
            }));
            this._greetSeqTimers.push(GLib.timeout_add(GLib.PRIORITY_DEFAULT, 3400, () => {
                this._eyeAnim(this._greetEyes._eyes, 'rainbow');
                return GLib.SOURCE_REMOVE;
            }));
        } else {
            // A friendly little eye sequence: wake → happy squint → wink → hearts.
            this._eyeAnim(this._greetEyes._eyes, 'surprised');
            this._greetSeqTimers.push(GLib.timeout_add(GLib.PRIORITY_DEFAULT, 650, () => {
                this._eyeAnim(this._greetEyes._eyes, 'squintHappy');
                return GLib.SOURCE_REMOVE;
            }));
            this._greetSeqTimers.push(GLib.timeout_add(GLib.PRIORITY_DEFAULT, 1500, () => {
                this._eyeAnim(this._greetEyes._eyes, 'wink');
                return GLib.SOURCE_REMOVE;
            }));
            this._greetSeqTimers.push(GLib.timeout_add(GLib.PRIORITY_DEFAULT, 2300, () => {
                this._eyeAnim(this._greetEyes._eyes, 'hearts');
                return GLib.SOURCE_REMOVE;
            }));
        }

        const hold = birthday ? 6500 : 4200;
        this._alertTimer = GLib.timeout_add(GLib.PRIORITY_DEFAULT, hold, () => {
            this._alertTimer = 0;
            this._greetBox.hide();
            this._clearGreetSeq();
            this._clearParticles();
            if (!this.hover) {
                this._closedBox.show();
                this._updateClosedVisual();
                this._animateTo(this._closed, this._tokens.motion.alertOut.durationMs, Clutter.AnimationMode.EASE_OUT_QUAD);
                this._armIdle();
                this._drainRoutines();
            } else {
                this.open();
            }
            return GLib.SOURCE_REMOVE;
        });
    }

    _clearGreetSeq() {
        for (const t of this._greetSeqTimers)
            if (t)
                GLib.source_remove(t);
        this._greetSeqTimers = [];
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
        this._tickRoutines();
        const now = GLib.DateTime.new_now_local();
        this._clock.text = now.format('%H:%M');
        if (this._date && this._open) {
            this._date.text = `${DAYS[now.get_day_of_week() % 7]} ${now.get_day_of_month()} ${MONTHS[now.get_month() - 1]} ${now.get_year()}`;
            this._time.text = now.format('%H:%M');
        }
    }

    // --- closed visualizer (only while playing) ---
    _startVisualizer() {
        if (this._vizTimer)
            return;
        const v = this._tokens.visualizer ?? {fps: 30, minHeight: 3, maxHeight: 14};
        const interval = Math.max(33, Math.round(1000 / (v.fps ?? 30)));
        this._vizTimer = GLib.timeout_add(GLib.PRIORITY_DEFAULT, interval, () => {
            for (const bar of this._viz._bars) {
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

    // Header visualizer: animates only while the notch is open.
    _startHeaderViz() {
        if (this._headerVizTimer)
            return;
        this._headerVizTimer = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 90, () => {
            const active = this._playing;
            for (const bar of this._headerViz._bars) {
                const h = active ? 3 + Math.random() * 13 : 3 + Math.random() * 4;
                bar.ease({height: h, duration: 90, mode: Clutter.AnimationMode.EASE_OUT_QUAD});
            }
            return GLib.SOURCE_CONTINUE;
        });
    }

    _stopHeaderViz() {
        if (this._headerVizTimer) {
            GLib.source_remove(this._headerVizTimer);
            this._headerVizTimer = 0;
        }
    }

    // --- water log (local hourly record for the chart) ---
    _waterLogPath() {
        const dir = GLib.build_filenamev([GLib.get_user_cache_dir(), 'mira']);
        GLib.mkdir_with_parents(dir, 0o755);
        const today = GLib.DateTime.new_now_local().format('%Y%m%d');
        return GLib.build_filenamev([dir, `water-${today}.json`]);
    }

    _readWaterLog() {
        const path = this._waterLogPath();
        let arr = new Array(24).fill(0);
        try {
            if (GLib.file_test(path, GLib.FileTest.EXISTS)) {
                const [ok, bytes] = GLib.file_get_contents(path);
                if (ok) {
                    const dec = new TextDecoder().decode(bytes);
                    const parsed = JSON.parse(dec);
                    if (Array.isArray(parsed) && parsed.length === 24)
                        arr = parsed;
                }
            }
        } catch (e) {
            // ignore, use zeros
        }
        this._waterLog = arr;
        return arr;
    }

    _logWater(ml) {
        const arr = this._readWaterLog();
        const h = GLib.DateTime.new_now_local().get_hour();
        arr[h] = Math.max(0, (arr[h] ?? 0) + ml);
        this._waterLog = arr;
        try {
            GLib.file_set_contents(this._waterLogPath(), JSON.stringify(arr));
        } catch (e) {
            // ignore
        }
        if (this._waterChart && this._waterChart.visible)
            this._waterChart.queue_repaint();
    }

    // --- mascot eyes: a small personality engine -----------------------------
    _armIdle() {
        this._updateClosedVisual();
    }

    _resetIdle() {
        this._updateClosedVisual();
    }

    _cancelIdle() {
        this._stopEyeLife();
    }

    _startEyeLife() {
        this._startEyeFollow();
        if (this._eyeLifeTimer)
            return;
        const schedule = () => {
            const delay = 1600 + Math.floor(Math.random() * 2600);
            this._eyeLifeTimer = GLib.timeout_add(GLib.PRIORITY_DEFAULT, delay, () => {
                this._playRandomEyeAnim();
                schedule();
                return GLib.SOURCE_REMOVE;
            });
        };
        schedule();
    }

    _stopEyeLife() {
        this._reminderGeneration++;
        this._reminderBadge.remove_all_transitions();
        this._reminderBadge.opacity = 255;
        this._stopEyeFollow();
        if (this._eyeLifeTimer) {
            GLib.source_remove(this._eyeLifeTimer);
            this._eyeLifeTimer = 0;
        }
        this._wakeEyeActors(this._eyes._eyes);
    }

    _playRandomEyeAnim() {
        if (this._reminderUrgency !== 'calm')
            return;
        const strength = this._settings.get_double('motion-intensity');
        if (strength <= 0) return;
        const mood = contextMood({urgency: this._reminderUrgency,
            focus: this._client.pomodoro().running, playing: this._playing,
            hour: new Date().getHours(), enabled: this._settings.get_boolean('mascot-contextual')});
        if (mood !== 'idle') {this._contextEyes(this._eyes._eyes, mood); return;}
        if (strength < 0.35) {this._blinkEyes(this._eyes._eyes); return;}
        const fixed = this._settings.get_string('eye-style');
        // With pointer-follow on, drop the plain look-around moves (the eyes
        // already track the mouse) and favour expressive ones.
        const all = this._eyeFollowWatch
            ? ['blink', 'blink', 'blink', 'doubleBlink', 'squintHappy', 'wink', 'surprised', 'excited', 'sleepy', 'love', 'hearts']
            : ['blink', 'blink', 'blink', 'doubleBlink', 'glanceL', 'glanceR', 'lookAround', 'lookUp', 'squintHappy', 'wink', 'surprised', 'excited', 'sleepy', 'dizzy', 'love', 'hearts'];
        let pick;
        if (fixed && fixed !== 'auto')
            pick = fixed;
        else
            pick = all[Math.floor(Math.random() * all.length)];
        this._playEyeAnimRested(pick);
    }

    // Play an animation on the resting eyes, pausing pointer-follow for the
    // moves that drive translation so the two don't fight.
    _playEyeAnimRested(name) {
        const busyMs = {glanceL: 900, glanceR: 900, lookAround: 1400, lookUp: 800, dizzy: 1000, excited: 1000}[name];
        if (busyMs) {
            this._eyeBusy = true;
            GLib.timeout_add(GLib.PRIORITY_DEFAULT, busyMs, () => {
                this._eyeBusy = false;
                return GLib.SOURCE_REMOVE;
            });
        }
        this._eyeAnim(this._eyes._eyes, name);
    }

    // Watch global pointer motion, including other monitors. GNOME suspends
    // this shared watcher while idle; Clutter animates only after a movement.
    _startEyeFollow() {
        if (this._eyeFollowWatch)
            return;
        this._eyeFollowWatch = PointerWatcher.getPointerWatcher().addWatch(16,
            (x, y) => this._updateEyeFollow(x, y));
        this._updateEyeFollow(...global.get_pointer());
    }

    _stopEyeFollow() {
        this._eyeFollowWatch?.remove();
        this._eyeFollowWatch = null;
        this._eyes.remove_all_transitions();
        this._eyes.translation_x = 0;
        this._eyes.translation_y = 0;
    }

    _updateEyeFollow(px, py) {
        if (this._open || !this._eyes.visible || !this.mapped)
            return;
        const [ax, ay] = this.get_transformed_position();
        const [w, h] = this.get_transformed_size();
        const target = gazeOffset(px - ax - w / 2, py - ay - h / 2);
        // Move the parent, so blinking and personality animations on each eye
        // compose with the gaze instead of overwriting its translation.
        this._eyes.ease({
            translation_x: target.x,
            translation_y: target.y,
            duration: 120,
            mode: Clutter.AnimationMode.EASE_OUT_QUAD,
        });
    }

    _eyeTimeout(delay, callback) {
        const id = GLib.timeout_add(GLib.PRIORITY_DEFAULT, delay, () => {
            this._eyeAnimTimers.delete(id);
            if (this._reminderAlive) callback();
            return GLib.SOURCE_REMOVE;
        });
        this._eyeAnimTimers.add(id);
        return id;
    }

    // The animation library. Each case animates the pair of eye actors.
    _eyeAnim(eyes, name) {
        if (!eyes || eyes.length < 2)
            return;
        const [l, r] = eyes;
        const base = l._baseH;
        const strength = this._settings.get_double('motion-intensity');
        if (strength <= 0) return;
        switch (name) {
        case 'blink':
            this._blinkEyes(eyes);
            break;
        case 'doubleBlink':
            this._blinkEyes(eyes);
            this._eyeTimeout(220, () => {
                this._blinkEyes(eyes);
                return GLib.SOURCE_REMOVE;
            });
            break;
        case 'glanceL':
            this._glanceEyes(eyes, -4 * strength);
            break;
        case 'glanceR':
            this._glanceEyes(eyes, 4 * strength);
            break;
        case 'lookAround':
            this._glanceEyes(eyes, -4 * strength);
            this._eyeTimeout(560, () => {
                this._glanceEyes(eyes, 4 * strength);
                return GLib.SOURCE_REMOVE;
            });
            break;
        case 'lookUp':
            for (const e of eyes)
                e.ease({translation_y: -3 * strength, duration: 260, mode: Clutter.AnimationMode.EASE_OUT_BACK,
                    onComplete: () => e.ease({translation_y: 0, duration: 420, mode: Clutter.AnimationMode.EASE_IN_OUT_QUAD})});
            break;
        case 'squintHappy':
            for (const e of eyes) {
                e.add_style_class_name('happy');
                e.ease({height: Math.max(3, base - 5 * strength), duration: 180, mode: Clutter.AnimationMode.EASE_OUT_QUAD,
                    onComplete: () => this._eyeTimeout(520, () => {
                        e.remove_style_class_name('happy');
                        e.ease({height: base, duration: 220, mode: Clutter.AnimationMode.EASE_OUT_QUAD});
                        return GLib.SOURCE_REMOVE;
                    })});
            }
            break;
        case 'wink':
            // Left eye winks, right stays.
            l.ease({height: 1, duration: 90, mode: Clutter.AnimationMode.EASE_IN_QUAD,
                onComplete: () => l.ease({height: base, duration: 160, mode: Clutter.AnimationMode.EASE_OUT_BACK})});
            break;
        case 'surprised':
            for (const e of eyes) {
                e.ease({scale_x: 1 + 0.5 * strength, scale_y: 1 + 0.5 * strength, duration: 160, mode: Clutter.AnimationMode.EASE_OUT_BACK,
                    onComplete: () => e.ease({scale_x: 1, scale_y: 1, duration: 320, mode: Clutter.AnimationMode.EASE_IN_OUT_QUAD})});
            }
            break;
        case 'excited':
            for (const e of eyes)
                this._bounce(e, 0);
            break;
        case 'sleepy':
            for (const e of eyes) {
                e.ease({height: Math.max(2, base - 6 * strength), duration: 600, mode: Clutter.AnimationMode.EASE_IN_OUT_QUAD,
                    onComplete: () => this._eyeTimeout(500, () => {
                        e.ease({height: base, duration: 500, mode: Clutter.AnimationMode.EASE_OUT_QUAD});
                        return GLib.SOURCE_REMOVE;
                    })});
            }
            break;
        case 'dizzy':
            this._dizzy(eyes, 0);
            break;
        case 'love':
            for (const e of eyes) {
                e.add_style_class_name('love');
                e.ease({scale_x: 1 + 0.35 * strength, scale_y: 1 + 0.35 * strength, duration: 220, mode: Clutter.AnimationMode.EASE_OUT_BACK,
                    onComplete: () => e.ease({scale_x: 1, scale_y: 1, duration: 300, mode: Clutter.AnimationMode.EASE_IN_OUT_QUAD,
                        onComplete: () => e.remove_style_class_name('love')})});
            }
            break;
        case 'hearts':
            for (const e of eyes) {
                e.add_style_class_name('love');
                e.ease({scale_x: 1 + 0.25 * strength, scale_y: 1 + 0.25 * strength, duration: 260, mode: Clutter.AnimationMode.EASE_OUT_BACK,
                    onComplete: () => e.ease({scale_x: 1, scale_y: 1, duration: 360, mode: Clutter.AnimationMode.EASE_IN_OUT_QUAD,
                        onComplete: () => e.remove_style_class_name('love')})});
            }
            this._spawnParticles('hearts', 7);
            break;
        case 'confetti':
            this._eyeAnim(eyes, 'surprised');
            this._spawnParticles('confetti', 40);
            break;
        case 'rainbow':
            this._rainbow(eyes, 0);
            break;
        default:
            this._blinkEyes(eyes);
        }
    }

    _rainbow(eyes, n) {
        const cols = ['#ff5a5a', '#ffb24d', '#ffe24d', '#2ee67a', '#3b9cff', '#9a6bff', '#ff7aa6'];
        if (n >= cols.length) {
            for (const e of eyes)
                e.set_style('');
            return;
        }
        for (const e of eyes)
            e.set_style(`background-color:${cols[n]}; box-shadow:0 0 8px 0 ${cols[n]};`);
        this._eyeTimeout(140, () => {
            this._rainbow(eyes, n + 1);
            return GLib.SOURCE_REMOVE;
        });
    }

    // --- particle effects (hearts, confetti) over the notch ---
    _spawnParticles(kind, count) {
        if (this._settings.get_double('motion-intensity') <= 0) return;
        const w = this.width || this._closed.w;
        const h = this.height || this._closed.h;
        for (let i = 0; i < count; i++) {
            const delay = Math.floor(Math.random() * (kind === 'confetti' ? 260 : 420));
            this._eyeTimeout(delay, () => {
                this._spawnOneParticle(kind, w, h);
                return GLib.SOURCE_REMOVE;
            });
        }
    }

    _spawnOneParticle(kind, w, h) {
        if (!this._fxLayer)
            return;
        // One actor, one transition each, so nothing touches a disposed object.
        let p;
        if (kind === 'hearts') {
            p = new St.Label({text: '❤', style_class: 'mira-heart'});
            const startX = w / 2 - 14 + (Math.random() * 28 - 14);
            p.set_position(startX, h * 0.5);
            p.set_pivot_point(0.5, 0.5);
            this._fxLayer.add_child(p);
            this._fxActors.push(p);
            p.ease({
                translation_y: -(h * 0.45 + Math.random() * 20),
                translation_x: (Math.random() * 24 - 12),
                scale_x: 1.4, scale_y: 1.4,
                opacity: 0,
                duration: 950 + Math.random() * 500,
                mode: Clutter.AnimationMode.EASE_OUT_QUAD,
                onComplete: () => this._removeParticle(p),
            });
        } else {
            // confetti square: falls, spins, fades; a single transition.
            const cols = ['#ff5a5a', '#ffb24d', '#ffe24d', '#2ee67a', '#3b9cff', '#9a6bff', '#ff7aa6', '#ffffff'];
            const c = cols[Math.floor(Math.random() * cols.length)];
            const sz = 5 + Math.floor(Math.random() * 5);
            p = new St.Widget({style: `background-color:${c}; border-radius:1px;`});
            p.set_size(sz, sz);
            p.set_pivot_point(0.5, 0.5);
            p.set_position(Math.random() * w, -6);
            p.rotation_angle_z = Math.random() * 360;
            this._fxLayer.add_child(p);
            this._fxActors.push(p);
            p.ease({
                translation_y: h + 14,
                translation_x: (Math.random() * 60 - 30),
                rotation_angle_z: p.rotation_angle_z + (Math.random() * 540 - 270),
                opacity: 0,
                duration: 1200 + Math.random() * 900,
                mode: Clutter.AnimationMode.EASE_IN_QUAD,
                onComplete: () => this._removeParticle(p),
            });
        }
    }

    _removeParticle(p) {
        const i = this._fxActors.indexOf(p);
        if (i >= 0)
            this._fxActors.splice(i, 1);
        if (p && !p._destroyed) {
            p._destroyed = true;
            p.remove_all_transitions();
            p.destroy();
        }
    }

    _clearParticles() {
        for (const p of this._fxActors.slice())
            this._removeParticle(p);
        this._fxActors = [];
    }

    _blinkEyes(eyes) {
        for (const e of eyes) {
            const base = e._baseH;
            e.ease({height: 1, duration: 90, mode: Clutter.AnimationMode.EASE_IN_QUAD,
                onComplete: () => e.ease({height: base, duration: 120, mode: Clutter.AnimationMode.EASE_OUT_QUAD})});
        }
    }

    _glanceEyes(eyes, offset) {
        for (const e of eyes) {
            e.ease({translation_x: offset, duration: 300, mode: Clutter.AnimationMode.EASE_OUT_BACK,
                onComplete: () => e.ease({translation_x: 0, duration: 480, mode: Clutter.AnimationMode.EASE_IN_OUT_QUAD})});
        }
    }

    _bounce(e, n) {
        if (n >= 3) {
            e.ease({translation_y: 0, duration: 140, mode: Clutter.AnimationMode.EASE_OUT_QUAD});
            return;
        }
        e.ease({translation_y: -4 * this._settings.get_double('motion-intensity'), duration: 130, mode: Clutter.AnimationMode.EASE_OUT_QUAD,
            onComplete: () => e.ease({translation_y: 0, duration: 130, mode: Clutter.AnimationMode.EASE_IN_QUAD,
                onComplete: () => this._bounce(e, n + 1)})});
    }

    _dizzy(eyes, n) {
        if (n >= 4) {
            for (const e of eyes)
                e.ease({translation_x: 0, duration: 200, mode: Clutter.AnimationMode.EASE_OUT_QUAD});
            return;
        }
        const off = (n % 2 === 0 ? 4 : -4) * this._settings.get_double('motion-intensity');
        for (const e of eyes)
            e.ease({translation_x: off, duration: 130, mode: Clutter.AnimationMode.EASE_IN_OUT_QUAD,
                onComplete: () => { if (e === eyes[0]) this._dizzy(eyes, n + 1); }});
    }

    _wakeEyeActors(eyes) {
        for (const e of eyes ?? []) {
            e.remove_all_transitions();
            e.set_height(e._baseH);
            e.translation_x = 0;
            e.translation_y = 0;
            e.scale_x = 1;
            e.scale_y = 1;
            e.opacity = 255;
            e.set_style('');
            e.remove_style_class_name('happy');
            e.remove_style_class_name('love');
        }
    }

    // --- settings / teardown ---
    applySettings() {
        this._themeEyes(this._eyes._eyes);
        this._themeEyes(this._greetEyes._eyes, true);
        this._computeSizes();
        const size = this._open ? this._hoverSize : this._closed;
        this.set_size(size.w, size.h);
        this._reposition(size.w);
        this._refreshMedia();
        this._resetIdle();
        this._scheduleGreeting();
        this._tickRoutines();
    }

    _onDestroy() {
        this._reminderAlive = false;
        this._summaryFetch++;
        if (this._naturalPreviewTimer) GLib.source_remove(this._naturalPreviewTimer);
        for (const id of this._eyeAnimTimers) GLib.source_remove(id);
        this._eyeAnimTimers.clear();
        this._reminderBadge.remove_all_transitions();
        this._reminderFetch++;
        if (this._reminderTimer)
            GLib.source_remove(this._reminderTimer);
        this._reminderTimer = 0;
        this._stopEyeFollow();
        for (const t of [
            this._closeTimer, this._alertTimer, this._clockTimer, this._vizTimer,
            this._headerVizTimer, this._progressTimer, this._greetTimer, this._eyeLifeTimer,
        ])
            if (t)
                GLib.source_remove(t);
        this._clearGreetSeq();
        this._clearParticles();
        if (this._monitorsChangedId) {
            Main.layoutManager.disconnect(this._monitorsChangedId);
            this._monitorsChangedId = 0;
        }
        this._closeTimer = this._alertTimer = this._clockTimer = this._vizTimer = 0;
        this._headerVizTimer = this._progressTimer = this._greetTimer = this._eyeLifeTimer = 0;
        this._eyeFollowWatch = null;
    }
});
