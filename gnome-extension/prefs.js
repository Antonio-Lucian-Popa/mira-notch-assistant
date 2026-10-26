// Mira preferences (Adwaita). Settings only — no business logic. Also reports
// whether the daemon is reachable on the session bus, since the notch needs it
// for everything beyond the clock.

import Adw from 'gi://Adw';
import Gio from 'gi://Gio';
import Gtk from 'gi://Gtk';

import {ExtensionPreferences, gettext as _} from 'resource:///org/gnome/Shell/Extensions/js/extensions/prefs.js';

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
