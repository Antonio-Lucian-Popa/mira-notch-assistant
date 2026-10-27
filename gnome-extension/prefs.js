// Mira preferences (Adwaita). Settings only — no business logic. Also reports
// whether the daemon is reachable on the session bus, since the notch needs it
// for everything beyond the clock.

import Adw from 'gi://Adw';
import Gio from 'gi://Gio';
import Gtk from 'gi://Gtk';

import {ExtensionPreferences, gettext as _} from 'resource:///org/gnome/Shell/Extensions/js/extensions/prefs.js';

import {validTime} from './lib/routines.js';
import {THEMES, EXPRESSIONS} from './lib/mascot.js';

const BUS_NAME = 'ro.novabytecode.Mira';
const OBJECT_PATH = '/ro/novabytecode/Mira';
const SERVICE_IFACE = 'ro.novabytecode.Mira.Service';

export default class MiraPreferences extends ExtensionPreferences {
    fillPreferencesWindow(window) {
        const settings = this.getSettings();

        const page = new Adw.PreferencesPage({title: _('Mira'), icon_name: 'preferences-system-symbolic'});
        window.add(page);

        // --- Daemon status ---
        const statusGroup = new Adw.PreferencesGroup({title: _('Daemon')});
        const statusRow = new Adw.ActionRow({
            title: _('Mira daemon'),
            subtitle: _('Checking…'),
        });
        statusGroup.add(statusRow);
        page.add(statusGroup);
        this._checkDaemon(statusRow);

        // --- Appearance ---
        const group = new Adw.PreferencesGroup({title: _('Appearance')});
        page.add(group);

        const modeRow = new Adw.ComboRow({
            title: _('Mode'),
            subtitle: _('Floating pill, or integrated into the panel clock area'),
            model: new Gtk.StringList({strings: [_('Floating'), _('Integrated')]}),
        });
        modeRow.selected = settings.get_string('mode') === 'integrated' ? 1 : 0;
        modeRow.connect('notify::selected', row =>
            settings.set_string('mode', row.selected === 1 ? 'integrated' : 'floating')
        );
        group.add(modeRow);

        const minimalRow = new Adw.SwitchRow({
            title: _('Minimal closed notch'),
            subtitle: _('Collapse the closed notch to a thin line'),
        });
        settings.bind('minimal', minimalRow, 'active', Gio.SettingsBindFlags.DEFAULT);
        group.add(minimalRow);

        const scaleRow = new Adw.SpinRow({
            title: _('Size'),
            subtitle: _('Manual size multiplier'),
            adjustment: new Gtk.Adjustment({lower: 0.8, upper: 1.6, step_increment: 0.05, page_increment: 0.1}),
            digits: 2,
        });
        settings.bind('size-scale', scaleRow, 'value', Gio.SettingsBindFlags.DEFAULT);
        group.add(scaleRow);

        const langRow = new Adw.ComboRow({
            title: _('Language'),
            model: new Gtk.StringList({strings: [_('Auto'), _('Română'), _('English')]}),
        });
        const langMap = ['auto', 'ro', 'en'];
        langRow.selected = Math.max(0, langMap.indexOf(settings.get_string('language')));
        langRow.connect('notify::selected', row =>
            settings.set_string('language', langMap[row.selected])
        );
        group.add(langRow);

        const mascotRow = new Adw.SwitchRow({
            title: _('Mascot'),
            subtitle: _('Use animated eyes as the resting face instead of the clock'),
        });
        settings.bind('mascot', mascotRow, 'active', Gio.SettingsBindFlags.DEFAULT);
        group.add(mascotRow);

        const eyeRow = new Adw.ComboRow({
            title: _('Eye personality'),
            subtitle: _('Which eye animation to loop (auto mixes them all)'),
            model: new Gtk.StringList({strings: [
                _('Auto (mix)'), 'Blink', 'Double blink', 'Glance', 'Look around',
                'Look up', 'Happy squint', 'Wink', 'Surprised', 'Excited', 'Sleepy', 'Dizzy', 'Love',
            ]}),
        });
        const eyeMap = ['auto', 'blink', 'doubleBlink', 'glanceR', 'lookAround', 'lookUp',
            'squintHappy', 'wink', 'surprised', 'excited', 'sleepy', 'dizzy', 'love'];
        eyeRow.selected = Math.max(0, eyeMap.indexOf(settings.get_string('eye-style')));
        eyeRow.connect('notify::selected', row => settings.set_string('eye-style', eyeMap[row.selected]));
        group.add(eyeRow);

        const routineGroup = new Adw.PreferencesGroup({title: _('Prânz și sfârșitul programului'), description: _('Ore locale, cu mesaje animate. Păstrează ultimele notificări chiar și după reconectare.')});
        page.add(routineGroup);
        for (const [key, title] of [['lunch-enabled', 'Anunță-mă la prânz'], ['work-end-enabled', 'Salută-mă la finalul programului'], ['routine-weekdays-only', 'Doar luni–vineri']]) {
            const row = new Adw.SwitchRow({title});
            settings.bind(key, row, 'active', Gio.SettingsBindFlags.DEFAULT);
            routineGroup.add(row);
        }
        for (const [key, title] of [['lunch-time', 'Ora prânzului (HH:MM)'], ['work-end-time', 'Termin munca la (HH:MM)']]) {
            const row = new Adw.EntryRow({title});
            row.text = settings.get_string(key);
            row.connect('changed', entry => {
                const valid = validTime(entry.text);
                entry[valid ? 'remove_css_class' : 'add_css_class']('error');
                if (valid) settings.set_string(key, valid);
            });
            routineGroup.add(row);
        }
        const lead = new Adw.SpinRow({title: _('Anunță-mă cu câte minute înainte de final'), adjustment: new Gtk.Adjustment({lower: 0, upper: 60, step_increment: 1}), digits: 0});
        settings.bind('work-end-lead-minutes', lead, 'value', Gio.SettingsBindFlags.DEFAULT);
        routineGroup.add(lead);
        const previews = new Adw.ActionRow({title: _('Previzualizează animațiile')});
        for (const [type, label] of [['lunch', 'Prânz 🍽'], ['work-soon', 'În curând'], ['work-end', 'La revedere 👋']]) {
            const button = new Gtk.Button({label, valign: Gtk.Align.CENTER});
            button.connect('clicked', () => settings.set_string('routine-preview', `${type}:${Date.now()}`));
            previews.add_suffix(button);
        }
        routineGroup.add(previews);

        const browserGroup = new Adw.PreferencesGroup({title: _('Muzică din browser'), description: _('Mira urmărește toate playerele MPRIS, inclusiv Brave, Chrome și Firefox.')});
        browserGroup.add(new Adw.ActionRow({title: _('Sesiuni private / Incognito'), subtitle: _('Browserul poate ascunde titlul și artistul. În pagina Lyrics poți introduce manual melodia pentru căutare pe LRCLIB.')}));
        page.add(browserGroup);

        const summaryGroup = new Adw.PreferencesGroup({title: _('Rezumatul zilei'), description: _('Doar activitatea înregistrată de Mira: focus, sarcini, remindere și apă. Fără scoruri sau comparații.')});
        page.add(summaryGroup);
        const summaryOn = new Adw.SwitchRow({title: _('Arată rezumatul zilnic')});
        settings.bind('daily-summary-enabled', summaryOn, 'active', Gio.SettingsBindFlags.DEFAULT);
        summaryGroup.add(summaryOn);
        const summaryTime = new Adw.EntryRow({title: _('Ora rezumatului (HH:MM)')});
        summaryTime.text = settings.get_string('daily-summary-time');
        summaryTime.connect('changed', row => {
            const time = validTime(row.text);
            row[time ? 'remove_css_class' : 'add_css_class']('error');
            if (time) settings.set_string('daily-summary-time', time);
        });
        summaryGroup.add(summaryTime);
        const summaryPreview = new Adw.ActionRow({title: _('Vezi rezumatul acum')});
        const summaryButton = new Gtk.Button({label: _('Previzualizează'), valign: Gtk.Align.CENTER});
        summaryButton.connect('clicked', () => settings.set_string('routine-preview', `summary:${Date.now()}`));
        summaryPreview.add_suffix(summaryButton); summaryGroup.add(summaryPreview);

        const studio = new Adw.PreferencesPage({title: _('Studio mascotă'), icon_name: 'applications-graphics-symbolic'});
        window.add(studio);
        const appearance = new Adw.PreferencesGroup({title: _('Teme și comportament'), description: _('Previzualizările rulează în notch, fără să modifice muzica sau timerul. Schimbările de temă și intensitate se salvează automat.')});
        studio.add(appearance);
        const themeNames = Object.keys(THEMES);
        const themeRow = new Adw.ComboRow({title: _('Tema mascotei'), model: new Gtk.StringList({strings: themeNames.map(name => THEMES[name].label)})});
        themeRow.selected = Math.max(0, themeNames.indexOf(settings.get_string('mascot-theme')));
        themeRow.connect('notify::selected', row => settings.set_string('mascot-theme', themeNames[row.selected]));
        appearance.add(themeRow);
        const contextual = new Adw.SwitchRow({title: _('Personalitate contextuală'), subtitle: _('Muzică → ritm discret; focus → concentrare; seara → somnolență. Reminderele urgente au prioritate.')});
        settings.bind('mascot-contextual', contextual, 'active', Gio.SettingsBindFlags.DEFAULT); appearance.add(contextual);
        const motion = new Adw.SpinRow({title: _('Intensitatea animațiilor'), subtitle: _('0 = expresii statice, 1 = mișcare completă'), adjustment: new Gtk.Adjustment({lower: 0, upper: 1, step_increment: 0.1}), digits: 1});
        settings.bind('motion-intensity', motion, 'value', Gio.SettingsBindFlags.DEFAULT); appearance.add(motion);
        const examples = new Adw.PreferencesGroup({title: _('Biblioteca de expresii'), description: _('Alege o expresie și privește notch-ul. La intensitate zero, mișcările sunt dezactivate.')});
        studio.add(examples);
        const grid = new Gtk.Grid({column_spacing: 8, row_spacing: 8, column_homogeneous: true});
        [...EXPRESSIONS, 'music', 'focus', 'evening', 'reminder'].forEach((name,index) => {
            const button = new Gtk.Button({label: name});
            button.connect('clicked', () => settings.set_string('mascot-preview', `${name}:${Date.now()}`));
            grid.attach(button,index % 3,Math.floor(index / 3),1,1);
        });
        examples.add(grid);

        // --- Greeting ---
        const greetGroup = new Adw.PreferencesGroup({
            title: _('Daily greeting'),
            description: _('A personal hello with the mascot eyes at your chosen time, plus birthday confetti.'),
        });
        page.add(greetGroup);

        const greetOnRow = new Adw.SwitchRow({
            title: _('Show daily greeting'),
        });
        settings.bind('greeting-enabled', greetOnRow, 'active', Gio.SettingsBindFlags.DEFAULT);
        greetGroup.add(greetOnRow);

        const nameRow = new Adw.EntryRow({title: _('Name')});
        nameRow.text = settings.get_string('greeting-name');
        nameRow.connect('changed', row => settings.set_string('greeting-name', row.text));
        greetGroup.add(nameRow);

        const timeRow = new Adw.EntryRow({title: _('Time (HH:MM)')});
        timeRow.text = settings.get_string('greeting-time');
        timeRow.connect('changed', row => {
            const t = row.text.trim();
            const valid = validTime(t);
            if (valid) settings.set_string('greeting-time', valid);
            row[valid ? 'remove_css_class' : 'add_css_class']('error');
        });
        greetGroup.add(timeRow);

        const bdayRow = new Adw.EntryRow({title: _('Birthday (MM-DD)')});
        bdayRow.text = settings.get_string('greeting-birthday');
        bdayRow.connect('changed', row => {
            const t = row.text.trim();
            if (t === '' || /^\d{1,2}-\d{1,2}$/.test(t))
                settings.set_string('greeting-birthday', t);
        });
        const bdayHint = new Gtk.Label({label: _('🎉'), valign: Gtk.Align.CENTER});
        bdayRow.add_suffix(bdayHint);
        greetGroup.add(bdayRow);

        const testRow = new Adw.ActionRow({
            title: _('Preview'),
            subtitle: _('Play the greeting now (hover away from the notch first)'),
        });
        const testBtn = new Gtk.Button({label: _('Greeting'), valign: Gtk.Align.CENTER});
        testBtn.connect('clicked', () => {
            settings.set_boolean('greeting-preview', !settings.get_boolean('greeting-preview'));
        });
        const bdayBtn = new Gtk.Button({label: _('Birthday 🎉'), valign: Gtk.Align.CENTER, css_classes: ['suggested-action']});
        bdayBtn.connect('clicked', () => {
            settings.set_boolean('greeting-preview-birthday', !settings.get_boolean('greeting-preview-birthday'));
        });
        testRow.add_suffix(testBtn);
        testRow.add_suffix(bdayBtn);
        greetGroup.add(testRow);
    }

    _checkDaemon(row) {
        try {
            const proxy = Gio.DBusProxy.new_sync(
                Gio.DBus.session,
                Gio.DBusProxyFlags.DO_NOT_AUTO_START,
                null,
                BUS_NAME,
                OBJECT_PATH,
                SERVICE_IFACE,
                null
            );
            if (proxy.g_name_owner) {
                const version = proxy.get_cached_property('Version');
                row.subtitle = _('Running') + (version ? ` (v${version.unpack()})` : '');
            } else {
                row.subtitle = _('Not running — start it with: systemctl --user start mira-daemon');
            }
        } catch (_e) {
            row.subtitle = _('Not running — start it with: systemctl --user start mira-daemon');
        }
    }
}
