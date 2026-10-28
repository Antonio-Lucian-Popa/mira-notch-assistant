// Session-only companion workbench. Native GTK drop surfaces bridge Wayland DND;
// the shell owns movement/positioning, clipboard events and visible interactions.
import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Meta from 'gi://Meta';
import St from 'gi://St';
import Pango from 'gi://Pango';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import * as PointerWatcher from 'resource:///org/gnome/shell/ui/pointerWatcher.js';

const WORK = ['shelf', 'notes', 'clipboard', 'search', 'phone'];
export class Desktop {
    constructor(notch, client, settings, hooks) {
        this.n = notch; this.client = client; this.settings = settings; this.hooks = hooks;
        this.alive = true; this.state = {links: [], roots: [], clips: [], capture: true, note: ''};
        this.pages = {}; this.timers = new Set(); this.handlers = []; this.sample = []; this.lastClick = 0;
        this.mode = settings.get_string('desktop-motion'); this.lastRoam = 0; this.nextRoam = 0;
        this.clipGeneration = 0; this.searchGeneration = 0; this.noteTimer = 0; this.refreshGeneration = 0;
        this.selection = global.display.get_selection(); this.cancel = new Gio.Cancellable();
    }
    later(ms, callback) {
        const id = GLib.timeout_add(GLib.PRIORITY_DEFAULT, ms, () => {this.timers.delete(id); if (this.alive) callback(); return GLib.SOURCE_REMOVE;});
        this.timers.add(id); return id;
    }
    button(text, fn, klass = '') {
        const b = new St.Button({child: new St.Label({text}), style_class: `mira-work-button ${klass}`, can_focus: true}); b.connect('clicked', fn); return b;
    }
    label(text, klass = 'mira-work-caption') {return new St.Label({text, style_class: klass});}
    build(parent) {
        for (const name of WORK) {
            const page = new St.BoxLayout({vertical: true, style_class: 'mira-work-page', x_expand: true, y_expand: true});
            this.pages[name] = page; parent.add_child(page); page.hide();
        }
        const shelf = this.pages.shelf;
        const actions = new St.BoxLayout({style_class: 'mira-work-actions'});
        actions.add_child(this.button('＋ Fișiere / folder', () => this.dropPad('choose')));
        actions.add_child(this.button('⇩ Primește drag & drop', () => this.dropPad('drop')));
        actions.add_child(this.button('Curăță sesiunea', () => this.clear())); shelf.add_child(actions);
        this.shelfStatus = this.label('Trage fișiere aici. Păstrăm shortcuturi, fără să mutăm originalele.'); shelf.add_child(this.shelfStatus);
        const scroll = new St.ScrollView({x_expand: true, y_expand: true, hscrollbar_policy: St.PolicyType.NEVER});
        this.cards = new St.BoxLayout({vertical: true, style_class: 'mira-work-cards', x_expand: true}); scroll.set_child(this.cards); shelf.add_child(scroll);
        const notes = this.pages.notes; notes.add_child(this.label('NOTIȚĂ TEMPORARĂ · Ctrl+V pentru text · se golește la închiderea sesiunii'));
        this.note = new St.Entry({style_class: 'mira-work-note', can_focus: true, x_expand: true, y_expand: true, hint_text: 'Lasă un gând, un link sau o bucată de cod…'});
        this.note.clutter_text.single_line_mode = false; this.note.clutter_text.line_wrap = true; this.note.clutter_text.activatable = false;
        this.note.clutter_text.connect('text-changed', () => {
            if (this.settingNote) return;
            if (this.noteTimer) { GLib.source_remove(this.noteTimer); this.timers.delete(this.noteTimer); }
            this.noteTimer = this.later(180, () => {this.noteTimer = 0; this.client.workspace('Note', [this.note.get_text().slice(0, 65536)]);});
        }); notes.add_child(this.note);
        notes.add_child(this.button('Golește notița', () => this.note.set_text('')));
        const clipActions = new St.BoxLayout({style_class: 'mira-work-actions'});
        this.capture = this.button('Capturare activă · Pauză', () => {
            this.state.capture = !this.state.capture; this.clipGeneration++;
            this.client.workspace('Capture', [this.state.capture], () => this.refresh());
        }); clipActions.add_child(this.capture); clipActions.add_child(this.button('Golește memoria temporară', () => this.clear()));
        this.pages.clipboard.add_child(clipActions);
        this.pages.clipboard.add_child(this.label('Ultimele 32 texte copiate · doar în RAM · click pentru a copia din nou.'));
        const clipScroll = new St.ScrollView({x_expand: true, y_expand: true});
        this.clips = new St.BoxLayout({vertical: true, style_class: 'mira-work-cards'}); clipScroll.set_child(this.clips); this.pages.clipboard.add_child(clipScroll);
        const search = this.pages.search;
        this.query = new St.Entry({hint_text: 'Caută text în folderele adăugate…', can_focus: true, style_class: 'mira-work-query'});
        this.query.clutter_text.connect('activate', () => this.search()); search.add_child(this.query);
        const searchActions = new St.BoxLayout({style_class: 'mira-work-actions'});
        searchActions.add_child(this.button('Caută ↵', () => this.search()));
        searchActions.add_child(this.button('Oprește', () => {this.searchGeneration++; this.client.workspace('CancelSearch', []); this.searchStatus.text = 'Căutare oprită.';})); search.add_child(searchActions);
        this.searchStatus = this.label('Grep literal, fără diferență între majuscule. Adaugă mai întâi un folder.'); search.add_child(this.searchStatus);
        const results = new St.ScrollView({x_expand: true, y_expand: true});
        this.results = new St.BoxLayout({vertical: true, style_class: 'mira-work-cards'}); results.set_child(this.results); search.add_child(results);
        this.buildPhone();
        this.remoteTimer=GLib.timeout_add(GLib.PRIORITY_DEFAULT,700,()=>{if(!this.alive)return GLib.SOURCE_REMOVE;if(this.phoneRunning)this.client.workspace('PhoneActions',[],jobs=>{if(this.alive&&Array.isArray(jobs))for(const job of jobs)this.executeRemote(job);});return GLib.SOURCE_CONTINUE;});
        this.menu = new PopupMenu.PopupMenu(this.n, .5, St.Side.TOP);
        Main.uiGroup.add_child(this.menu.actor); this.menu.actor.hide();
        this.menu.actor.connect('destroy',()=>{this.menuDestroyed=true;});
        this.menuManager = new PopupMenu.PopupMenuManager(this.n); this.menuManager.addMenu(this.menu);
        for (const [label, mode] of [['Urmărește mouse-ul · întârziere 0.5s', 'follow'], ['Pin · rămâi aici', 'pinned'], ['Free roam · explorează', 'roam'], ['Acasă · înapoi sus', 'docked']])
            this.menu.addAction(label, () => mode === 'docked' ? this.home() : this.setMode(mode));
        this.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        for (const [label, page] of [['Raft de fișiere', 'shelf'], ['Sticky notepad', 'notes'], ['Clipboard', 'clipboard'], ['Caută în foldere', 'search']]) this.menu.addAction(label, () => this.openPage(page));
        this.menu.addAction('Telefon · asociere și taste',()=>this.openPage('phone'));
        this.menu.addAction('Alege fișiere sau folder', () => this.dropPad('choose'));
        this.menu.addAction('Mai mare ＋', () => this.resizeBy(1.1)); this.menu.addAction('Mai mic −', () => this.resizeBy(.9));
        this.menu.addAction('Pauză / reia clipboard', () => {this.state.capture = !this.state.capture; this.client.workspace('Capture', [this.state.capture], () => this.refresh());});
        this.menu.addAction('Șterge memoria temporară', () => this.clear());
        // Resize grip only makes sense on the detached (floating) workspace; in
        // the default docked notch it is hidden so the pill stays clean.
        this.grip = new St.Label({text: '◢', style_class: 'mira-work-grip', reactive: true, x_align: Clutter.ActorAlign.END, y_align: Clutter.ActorAlign.END});
        this.grip.visible = this.mode !== 'docked';
        const footer = new St.BoxLayout({x_expand:true});
        this.n._openBox.remove_child(this.n._dots);
        footer.add_child(new St.Widget({x_expand:true})); footer.add_child(this.n._dots);
        footer.add_child(new St.Widget({x_expand:true})); footer.add_child(this.grip);
        this.n._openBox.add_child(footer);
        this.handlers.push([global.stage, global.stage.connect('captured-event', (_stage, e) => this.event(e))]);
        this.handlers.push([this.selection, this.selection.connect('owner-changed', (_s, type) => {if (type === Meta.SelectionType.CLIPBOARD) this.clipboard();})]);
        this.handlers.push([global.display, global.display.connect('window-created', (_display, window) => this.positionPad(window))]);
        if (Main.xdndHandler) {
            this.handlers.push([Main.xdndHandler, Main.xdndHandler.connect('drag-begin', () => {this.externalDrag = true;})]);
            this.handlers.push([Main.xdndHandler, Main.xdndHandler.connect('drag-end', () => {this.externalDrag = false; this.padLaunched = false; this.refresh();})]);
        }
        this.pointer = PointerWatcher.getPointerWatcher().addWatch(35, (x, y) => {
            this.pointerAt = [x, y]; this.sample.push([GLib.get_monotonic_time()/1000, x, y]);
            if (this.sample.length > 40) this.sample.shift();
            if (this.externalDrag && !this.padLaunched && this.inside(x, y)) {this.padLaunched = true; this.openPage('shelf'); this.dropPad('drop');}
        });
        this.loop = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 40, () => {if (!this.alive) return GLib.SOURCE_REMOVE; this.tick(); return GLib.SOURCE_CONTINUE;});
        this.refresh();
    }
    openPage(name) { this.n.setPage(this.n._pageNames.indexOf(name)); this.n.open(); this.refresh(); }
    setMode(mode) {this.mode = mode; this.settings.set_string('desktop-motion', mode); this.n.add_style_class_name('mira-floating'); if (this.grip) this.grip.visible = mode !== 'docked';}
    home() {
        this.setMode('docked'); this.drag = null; this.grab?.dismiss(); this.grab = null;
        if (this.grip) this.grip.visible = false;
        this.n.close(); this.n._reposition(this.n._closed.w); this.n.remove_style_class_name('mira-floating');
    }
    bounds(x, y) {return Main.layoutManager.monitors.find(m => x >= m.x && x < m.x+m.width && y >= m.y && y < m.y+m.height) ?? Main.layoutManager.primaryMonitor;}
    inside(x, y) {return x >= this.n.x && x <= this.n.x+this.n.width && y >= this.n.y && y <= this.n.y+this.n.height;}
    move(x, y) {
        const m = this.bounds(x, y); if (!m) return;
        this.n.set_position(Math.max(m.x, Math.min(m.x+m.width-this.n.width, x)), Math.max(m.y+30, Math.min(m.y+m.height-this.n.height, y)));
    }
    tick() {
        if (this.drag || this.menu.isOpen || this.externalDrag || this.n._open || Main.sessionMode.isLocked) return;
        const [px, py, mask] = global.get_pointer(); if (mask & Clutter.ModifierType.BUTTON1_MASK || this.inside(px, py)) return;
        const now = GLib.get_monotonic_time()/1000;
        if (this.mode === 'follow') {
            while (this.sample.length > 1 && this.sample[1][0] <= now-500) this.sample.shift();
            const sample = this.sample[0];
            if (sample && sample[0] <= now-500) {
                const x = sample[1]-this.n.width/2, y = sample[2]+30;
                if (Math.abs(x-this.n.x)+Math.abs(y-this.n.y) > 2) this.move(this.n.x+(x-this.n.x)*.14, this.n.y+(y-this.n.y)*.14);
            }
        } else if (this.mode === 'roam' && now >= this.nextRoam) {
            this.nextRoam = now + 7000 + Math.random()*10000;
            const m = this.bounds(this.n.x, this.n.y); if (!m) return;
            const x = m.x+24+Math.random()*Math.max(0,m.width-this.n.width-48), y = m.y+50+Math.random()*Math.max(0,m.height-this.n.height-100);
            if (this.settings.get_double('motion-intensity') > 0) this.n.ease({x, y, duration: 1700+Math.random()*1300, mode: Clutter.AnimationMode.EASE_IN_OUT_QUAD});
            this.n._playEyeAnimRested(['sleepy','lookAround','doubleBlink','excited'][Math.floor(Math.random()*4)]);
            this.think();
        }
    }
    think() {
        // Actual Linux counters; percentage is sampled over the previous roam visit.
        try {
            const decode = p => new TextDecoder().decode(GLib.file_get_contents(p)[1]);
            const cpu = decode('/proc/stat').split('\n')[0].trim().split(/\s+/).slice(1).map(Number);
            const total = cpu.reduce((a,b)=>a+b,0), idle = cpu[3]+cpu[4];
            const mem = decode('/proc/meminfo'); const totalMem = Number(/MemTotal:\s+(\d+)/.exec(mem)?.[1]); const available = Number(/MemAvailable:\s+(\d+)/.exec(mem)?.[1]);
            const cpuPct = this.previousCpu ? Math.round(100*(1-(idle-this.previousCpu.idle)/Math.max(1,total-this.previousCpu.total))) : null;
            this.previousCpu={total,idle};
            const msg = `${cpuPct === null ? 'Verific sistemul' : `${cpuPct}% CPU`} · ${Math.round(100*(1-available/totalMem))}% RAM`;
            this.n._clock.text = `◌ ${msg}`; this.n._clock.show(); this.n._eyes.show();
            this.later(4000, () => this.n._updateClosedVisual());
        } catch { /* procfs unavailable: keep the mascot; don't invent statistics. */ }
    }
    event(e) {
        if (!this.alive || Main.sessionMode.isLocked || !this.n.visible) return Clutter.EVENT_PROPAGATE;
        const type = e.type(), [x,y] = e.get_coords();
        if (type === Clutter.EventType.BUTTON_PRESS && this.inside(x,y)) {
            if (e.get_button() === 3) {this.menu.toggle(); return Clutter.EVENT_STOP;}
            if (e.get_button() !== 1) return Clutter.EVENT_PROPAGATE;
            const source = e.get_source();
            if (this.n._tabsBox?.contains(source) || this.n._extraTabs?.contains(source)) return Clutter.EVENT_PROPAGATE;
            const now = e.get_time();
            if (now-this.lastClick < 360) {this.lastClick=0; this.home(); return Clutter.EVENT_STOP;}
            const resizing = this.n._open && x > this.n.x+this.n.width-26 && y > this.n.y+this.n.height-26;
            if (!resizing && this.n._open && source !== this.n && !this.n._header.contains(source)) return Clutter.EVENT_PROPAGATE;
            this.drag={x,y,ax:this.n.x,ay:this.n.y,w:this.n.width,h:this.n.height,kind:resizing?'resize':'move',moved:false,lastY:y,lastTime:now,vy:0};
            this.n.remove_all_transitions(); this.grab = global.stage.grab(this.n); return Clutter.EVENT_STOP;
        }
        if (this.drag && type === Clutter.EventType.MOTION) {
            const d=this.drag, dx=x-d.x, dy=y-d.y; d.vy=(y-d.lastY)/Math.max(1,e.get_time()-d.lastTime); d.lastY=y; d.lastTime=e.get_time();
            if (Math.abs(dx)+Math.abs(dy)<6 && !d.moved) return Clutter.EVENT_STOP;
            d.moved=true; this.setMode('pinned');
            if (d.kind==='resize') {
                this.n._hoverSize={w:Math.max(440,Math.min(960,d.w+dx)),h:Math.max(260,Math.min(760,d.h+dy)),r:26};
                this.n.set_size(this.n._hoverSize.w,this.n._hoverSize.h);
            } else {
                if (dy>14 && !this.n._open) this.openPage('shelf');
                this.n.remove_all_transitions(); this.move(d.ax+dx,d.ay+dy);
            }
            return Clutter.EVENT_STOP;
        }
        if (this.drag && type === Clutter.EventType.BUTTON_RELEASE) {
            const d=this.drag; this.drag=null; this.grab?.dismiss(); this.grab=null;
            if (d.moved) {
                if (d.kind==='resize') {
                    this.settings.set_int('workspace-width',Math.round(this.n.width)); this.settings.set_int('workspace-height',Math.round(this.n.height));
                } else if (d.vy < -1.1 || y < (this.bounds(x,y)?.y ?? 0)+55) this.home();
                else this.folderUnder(x,y);
            } else {this.lastClick=e.get_time(); this.n.open();}
            return Clutter.EVENT_STOP;
        }
        return Clutter.EVENT_PROPAGATE;
    }
    resizeBy(factor) {
        this.settings.set_double('size-scale',Math.max(.8,Math.min(1.6,this.settings.get_double('size-scale')*factor)));
        this.settings.set_int('workspace-width',Math.round(Math.max(440,Math.min(960,this.settings.get_int('workspace-width')*factor))));
        this.settings.set_int('workspace-height',Math.round(Math.max(260,Math.min(760,this.settings.get_int('workspace-height')*factor)))); this.n.open();
    }
    folderUnder(x,y) {
        // Desktop extensions that expose Gio.File/URI delegates can identify an icon.
        // Nautilus/DING process surfaces cannot be guessed from a window title.
        this.n.hide(); let actor=global.stage.get_actor_at_pos(Clutter.PickMode.REACTIVE,x,y); this.n.show();
        while (actor) {
            const d=actor._delegate; const file=d?.file ?? d?._file; const uri=d?.uri ?? d?._uri;
            try {const f=file?.get_path ? file : uri?.startsWith('file://') ? Gio.File.new_for_uri(uri) : null;
                if (f?.query_file_type(Gio.FileQueryInfoFlags.NOFOLLOW_SYMLINKS,null)===Gio.FileType.DIRECTORY) {this.add([f.get_path()]); return;}
            } catch { /* The hovered delegate may disappear during a workspace switch. */ }
            actor=actor.get_parent();
        }
    }
    clipboard() {
        if (!this.state.capture || Main.sessionMode.isLocked) return;
        const gen=++this.clipGeneration;
        St.Clipboard.get_default().get_text(St.ClipboardType.CLIPBOARD, (_cb,text) => {
            if (!this.alive || gen!==this.clipGeneration || !this.state.capture || !text) return;
            this.client.workspace('PushClip',[Array.from(text).slice(0,16384).join('')],()=>{if (this.n._pageNames[this.n._page]==='clipboard') this.refresh();});
        });
    }
    add(paths) {this.client.workspace('AddPaths',[paths],state=>{if (!this.alive) return; this.state=state; this.render(); this.openPage('shelf');});}
    refresh() {
        if(this.n._pageNames[this.n._page]==='phone'){this.client.workspace('PhoneState',[],data=>this.phoneRender(data));}
        const gen=++this.refreshGeneration;
        this.client.workspace('Snapshot',[],state=>{if (!this.alive || gen!==this.refreshGeneration || state.error) return;
            this.state=state; this.render();
            if (!this.importedLegacy && this.client.getShelf) {this.importedLegacy=true; this.client.getShelf(items=>{if (this.alive && items.length) this.add(items.map(i=>i[1]));});}
            if (!this.note.has_key_focus() && this.note.get_text()!==state.note) {this.settingNote=true; this.note.set_text(state.note??''); this.settingNote=false;}
        });
    }
    render() {
        this.cards.destroy_all_children(); this.clips.destroy_all_children();
        this.capture.get_child().text=this.state.capture?'Capturare activă · Pauză':'Capturare oprită · Reia';
        this.shelfStatus.text=this.state.message||'Trage fișiere aici sau alege un folder. Originalele rămân la locul lor.';
        for (const link of this.state.links??[]) {
            const row=new St.BoxLayout({style_class:'mira-work-file',x_expand:true});
            const open=this.button('',()=>Gio.AppInfo.launch_default_for_uri(Gio.File.new_for_path(link.path).get_uri(),global.create_app_launch_context(0,-1)));
            const content=new St.BoxLayout({style_class:'mira-work-file-inner',x_expand:true});
            content.add_child(new St.Icon({icon_name:link.directory?'folder-symbolic':'text-x-generic-symbolic',icon_size:25,style_class:'mira-work-file-icon'}));
            const name=this.label(link.name,'mira-work-file-name'); name.clutter_text.ellipsize=3; name.x_expand=true; content.add_child(name); open.set_child(content); open.x_expand=true;
            open.connect('notify::hover',()=>{
                if (this.settings.get_double('motion-intensity')>0) row.ease({translation_y:open.hover?-3:0,duration:380,mode:Clutter.AnimationMode.EASE_OUT_BACK});
            });
            row.add_child(open); row.add_child(this.button('×',()=>this.client.workspace('Remove',[link.path],()=>this.refresh()))); this.cards.add_child(row);
        }
        if (!(this.state.links??[]).length) this.cards.add_child(this.label('Un raft gol, pregătit pentru ce ai la îndemână.'));
        for (const clip of this.state.clips??[]) {
            const b=this.button(clip.replace(/\s+/g,' ').slice(0,130),()=>St.Clipboard.get_default().set_text(St.ClipboardType.CLIPBOARD,clip),'mira-work-clip');
            b.get_child().clutter_text.ellipsize=3; this.clips.add_child(b);
        }
    }
    clear() {
        if (this.noteTimer) {GLib.source_remove(this.noteTimer); this.timers.delete(this.noteTimer); this.noteTimer=0;}
        this.clipGeneration++; this.searchGeneration++; this.query.set_text(''); this.results.destroy_all_children();
        this.client.workspace('Clear',[],()=>this.refresh());
    }
    search() {
        const gen=++this.searchGeneration; const query=this.query.get_text();
        this.searchStatus.text='Caut…'; this.results.destroy_all_children();
        this.client.workspace('Search',[query],result=>{
            if (!this.alive || gen!==this.searchGeneration) return;
            this.searchStatus.text=result.error??`${result.hits?.length??0} potriviri · ${result.files??0} fișiere${result.limited?' · rezultate limitate':''}`;
            for (const hit of result.hits??[]) {
                const b=this.button(`${GLib.path_get_basename(hit.path)}:${hit.line}  ${hit.text}`,()=>Gio.AppInfo.launch_default_for_uri(Gio.File.new_for_path(hit.path).get_uri(),global.create_app_launch_context(0,-1)),'mira-work-hit');
                b.get_child().clutter_text.ellipsize=3; this.results.add_child(b);
            }
        });
    }
    buildPhone(){
        const page=this.pages.phone;
        const heading = new St.BoxLayout({style_class: 'mira-phone-heading', x_expand: true});
        const titles = new St.BoxLayout({vertical: true, x_expand: true, style_class: 'mira-phone-titles'});
        titles.add_child(this.label('Telefonul tău, aproape.', 'mira-section-title'));
        titles.add_child(this.label('Transcrieri și acțiuni, în aceeași rețea.'));
        heading.add_child(titles);
        this.phoneBadge = this.label('● Oprit', 'mira-status-badge');
        this.phoneBadge.y_align = Clutter.ActorAlign.START;
        this.phoneBadge.y_expand = false;
        heading.add_child(this.phoneBadge);
        page.add_child(heading);
        const sections = new St.BoxLayout({style_class: 'mira-phone-sections', x_expand: true});
        this.phonePanels = [];
        this.phoneSectionButtons = [];
        const selectSection = index => {
            this.phonePanels.forEach((panel, i) => {panel.visible = i === index;});
            this.phoneSectionButtons.forEach((button, i) => {
                if (i === index) button.add_style_class_name('active');
                else button.remove_style_class_name('active');
            });
            if (this.n._contentScroll) this.n._contentScroll.vscroll.adjustment.value = 0;
        };
        ['Asociere', 'Shortcuturi', 'Administrare'].forEach((name, index) => {
            const button = this.button(name, () => selectSection(index), 'mira-phone-section');
            button.x_expand = true;
            sections.add_child(button);
            this.phoneSectionButtons.push(button);
        });
        page.add_child(sections);
        this.phoneFeedback = this.label('', 'mira-phone-feedback');
        this.phoneFeedback.clutter_text.single_line_mode = false;
        this.phoneFeedback.clutter_text.line_wrap = true;
        this.phoneFeedback.clutter_text.ellipsize = Pango.EllipsizeMode.NONE;
        this.phoneFeedback.hide();
        page.add_child(this.phoneFeedback);
        const connection = new St.BoxLayout({vertical: true, style_class: 'mira-surface', x_expand: true});
        const bar = new St.BoxLayout({style_class: 'mira-work-actions'});
        this.phoneStart = this.button('Pornește primirea', () => this.client.workspace('PhoneStart', [], data => this.phoneRender(data)), 'mira-button-primary');
        this.phoneStop = this.button('Oprește primirea', () => this.client.workspace('PhoneStop', [], () => this.client.workspace('PhoneState', [], data => this.phoneRender(data))));
        bar.add_child(this.phoneStart);
        bar.add_child(this.phoneStop);
        this.phoneStop.hide();
        this.phonePdf = this.button('Deschide PDF-uri', () => {if (this.phoneFolder) Gio.AppInfo.launch_default_for_uri(Gio.File.new_for_path(this.phoneFolder).get_uri(), global.create_app_launch_context(0, -1));});
        bar.add_child(this.phonePdf);
        connection.add_child(bar);
        this.phoneInfo = this.label('Pornește primirea pentru a asocia telefonul.');
        this.phoneInfo.clutter_text.single_line_mode = false;
        this.phoneInfo.clutter_text.line_wrap = true;
        this.phoneInfo.clutter_text.line_wrap_mode = Pango.WrapMode.WORD_CHAR;
        this.phoneInfo.clutter_text.ellipsize = Pango.EllipsizeMode.NONE;
        connection.add_child(this.phoneInfo);
        const row = new St.BoxLayout({style_class: 'mira-phone-pairing'});
        this.qr=new St.DrawingArea();this.qr.set_size(160,160);this.qr.connect('repaint',()=>{
            if(!this.qrMatrix)return;const cr=this.qr.get_context();const [w,h]=this.qr.get_surface_size();const size=this.qrMatrix.length;const step=Math.min(w,h)/(size+8);cr.setSourceRGB(1,1,1);cr.paint();cr.setSourceRGB(0,0,0);
            this.qrMatrix.forEach((line,y)=>line.forEach((bit,x)=>{if(bit){cr.rectangle((x+4)*step,(y+4)*step,Math.ceil(step),Math.ceil(step));cr.fill();}}));cr.$dispose();
        });this.qr.hide();row.add_child(this.qr);
        const setup=new St.BoxLayout({vertical:true,style_class:'mira-phone-setup',x_expand:true});
        this.phoneCopy = this.button('Copiază codul de asociere',()=>{if(this.pairCode)St.Clipboard.get_default().set_text(St.ClipboardType.CLIPBOARD,this.pairCode);});
        setup.add_child(this.phoneCopy);
        this.phoneCopy.hide();
        row.add_child(setup); connection.add_child(row); page.add_child(connection);
        this.phoneManage = new St.BoxLayout({vertical:true, style_class:'mira-surface'});
        this.phoneManage.add_child(this.label('Revocarea deconectează telefonul asociat.'));
        this.phoneManage.add_child(this.button('Revocă asocierea',()=>this.client.workspace('PhoneRevoke',[],()=>this.client.workspace('PhoneState',[],d=>this.phoneRender(d))), 'mira-button-danger'));
        this.phoneManage.hide(); page.add_child(this.phoneManage);
        const shortcuts = new St.BoxLayout({vertical:true,style_class:'mira-surface'});
        shortcuts.add_child(this.label('Shortcuturi și scuturare', 'mira-section-subtitle'));
        const form = new St.BoxLayout({vertical:true,style_class:'mira-phone-setup'});
        shortcuts.add_child(form); page.add_child(shortcuts);
        this.actionName=new St.Entry({hint_text:'Numele acțiunii',can_focus:true,style_class:'mira-work-query'});
        this.actionKeys=new St.Entry({hint_text:'CTRL+ALT+2',can_focus:true,style_class:'mira-work-query'});form.add_child(this.actionName);form.add_child(this.actionKeys);
        form.add_child(this.button('Adaugă shortcut',()=>this.client.workspace('PhoneAddAction',[this.actionName.get_text(),this.actionKeys.get_text()],data=>{if(data.error)this.phoneError(data.error);else this.client.workspace('PhoneState',[],d=>this.phoneRender(d));})));
        this.phoneList=new St.BoxLayout({vertical:true,style_class:'mira-shortcut-list'});shortcuts.add_child(this.phoneList);
        this.phonePanels = [connection, shortcuts, this.phoneManage];
        selectSection(0);
    }
    phoneError(message) {
        if (!this.alive) return;
        this.phoneFeedback.text = message;
        this.phoneFeedback.show();
    }
    phoneRender(data){
        if(!this.alive)return;
        if(data.error){this.phoneError(data.error);return;}
        this.phoneFeedback.hide();
        this.phoneFolder=data.directory;this.phoneRunning=!!data.running;this.pairCode=data.code;this.qrMatrix=data.qr;this.qr.visible=!!data.running;this.qr.queue_repaint();this.phoneList.destroy_all_children();
        this.phoneBadge.text = data.running ? '● Primire activă' : '● Oprit';
        this.phoneBadge.set_style_class_name(`mira-status-badge${data.running ? ' online' : ''}`);
        this.phoneStart.visible = !data.running;
        this.phoneStop.visible = !!data.running;
        this.phoneCopy.visible = !!data.running && !!data.code;
        this.phonePdf.reactive = !!data.directory;
        this.phonePdf.can_focus = !!data.directory;
        this.phoneInfo.text = data.running ? `Scanează codul cu Mira de pe telefon. Adresă locală: ${data.address ?? ''}` : 'Pornește primirea și scanează codul QR din Mira de pe telefon. Folosește aceeași rețea Wi-Fi.';
        const shake=data.shake??{first:'lock',second:'',next_second:false};
        const update=(first,second)=>this.client.workspace('PhoneSetShake',[first,second],result=>{if(result.error)this.phoneError(result.error);else this.client.workspace('PhoneState',[],d=>this.phoneRender(d));});
        const status=this.label(!shake.first&&!shake.second?'Scuturare inactivă · alege o acțiune':`Scuturare · urmează ${shake.next_second||!shake.first?'revenirea':'plecarea'} · salvat pe PC`);
        status.clutter_text.line_wrap=true;this.phoneList.add_child(status);
        this.phoneList.add_child(this.button('Resetează la plecare',()=>update(shake.first,shake.second)));
        for(const action of data.actions??[]){
            const card=new St.BoxLayout({vertical:true,style_class:'mira-shake-action'});
            const title=this.label(action.name, 'mira-shortcut-name');
            card.add_child(this.label(action.lock ? 'Blocare PC' : action.keys.join(' + '), 'mira-keycap'));
            title.clutter_text.ellipsize=3;card.add_child(title);
            const choices=new St.BoxLayout({style_class:'mira-shake-choices'});
            choices.add_child(this.button(shake.first===action.id?'✓ Plecare':'Plecare',()=>update(action.id,shake.second)));
            choices.add_child(this.button(shake.second===action.id?'✓ Revenire':'Revenire',()=>update(shake.first,action.id)));
            choices.add_child(this.button('×',()=>this.client.workspace('PhoneRemoveAction',[action.id],result=>{if(result.error)this.phoneError(result.error);else this.client.workspace('PhoneState',[],d=>this.phoneRender(d));})));
            card.add_child(choices);this.phoneList.add_child(card);
        }
        this.phoneList.add_child(this.button('Doar plecare · fără alternare',()=>update(shake.first,'')));
        this.phoneList.add_child(this.button('Dezactivează acțiunile la scuturare',()=>update('','')));
    }
    executeRemote(job){
        if(Date.now()-job.issued_at>10000)return;
        if(job.action.lock){Main.screenShield.lock(true);return;}
        if(Main.sessionMode.isLocked && job.action.keys.length < 2)return;
        this.virtualKeyboard??=Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
        const special={CTRL:Clutter.KEY_Control_L,ALT:Clutter.KEY_Alt_L,SHIFT:Clutter.KEY_Shift_L,SUPER:Clutter.KEY_Super_L,ENTER:Clutter.KEY_Return,TAB:Clutter.KEY_Tab,SPACE:Clutter.KEY_space,ESC:Clutter.KEY_Escape,DELETE:Clutter.KEY_Delete,BACKSPACE:Clutter.KEY_BackSpace,HOME:Clutter.KEY_Home,END:Clutter.KEY_End,LEFT:Clutter.KEY_Left,RIGHT:Clutter.KEY_Right,UP:Clutter.KEY_Up,DOWN:Clutter.KEY_Down,PAGEUP:Clutter.KEY_Page_Up,PAGEDOWN:Clutter.KEY_Page_Down,PLUS:Clutter.KEY_plus,MINUS:Clutter.KEY_minus};
        const keys=job.action.keys.map(key=>special[key]??(/^F([1-9]|1[0-9]|2[0-4])$/.test(key)?Clutter[`KEY_${key}`]:key.toLowerCase().charCodeAt(0)));
        const time=GLib.get_monotonic_time();const pressed=[];try{for(const key of keys){this.virtualKeyboard.notify_keyval(time,key,Clutter.KeyState.PRESSED);pressed.push(key);}}finally{for(const key of pressed.reverse())this.virtualKeyboard.notify_keyval(time+1000,key,Clutter.KeyState.RELEASED);}
    }
    dropPad(mode) {
        this.openPage('shelf');
        try {
            if (this.padProcess) return;
            this.padProcess=Gio.Subprocess.new(['gjs','-m',`${this.hooks.extensionPath}/lib/drop-pad.js`,mode],Gio.SubprocessFlags.NONE);
            this.padProcess.wait_async(null,(process,result)=>{try{process.wait_finish(result);}catch{}this.padProcess=null;if(this.alive){this.n.show();this.refresh();if(!process.get_successful())this.shelfStatus.text='Zona de drop nu este disponibilă în această sesiune. Poți adăuga fișiere cu Ctrl+V.';}});
        } catch (e) {this.shelfStatus.text=`Nu pot deschide zona de drop: ${e.message}`;}
    }
    positionPad(window) {
        // Wayland app_id is populated after window-created, not necessarily at emission.
        this.later(180,()=>{
            if (window.get_gtk_application_id()!=='ro.novabytecode.Mira.DropPad') return;
            this.n.hide();
            window.make_above();
            window.move_resize_frame(true,Math.round(this.n.x),Math.round(this.n.y),Math.round(this.n.width),Math.round(this.n.height));
        });
    }
    destroy() {
        this.alive=false; this.cancel.cancel();
        if(this.remoteTimer)GLib.source_remove(this.remoteTimer);this.client.workspace('PhoneStop',[]);this.virtualKeyboard=null; this.grab?.dismiss();
        this.pointer?.remove(); if (this.loop) GLib.source_remove(this.loop);
        for (const id of this.timers) GLib.source_remove(id);
        for (const [obj,id] of this.handlers) obj.disconnect(id);
        if (!this.menuDestroyed) this.menu.destroy(); this.padProcess?.force_exit();
        // The daemon's workspace is per-session and explicitly cleared on extension disable.
        this.client.workspace('Clear',[]);
    }
}
