// Mira GNOME Shell extension entry point.
//
// Adds the notch as top chrome, connects it to the Mira daemon over D-Bus, and
// handles fullscreen hiding and the integrated/floating modes. If the daemon is
// not running the notch still shows the clock (a graceful minimal fallback) and
// the preferences window explains how to start it.

import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

import {MiraClient} from './lib/dbus.js';
import {Notch} from './lib/notch.js';
import {loadTokens} from './lib/tokens.js';

export default class MiraExtension extends Extension {
    enable() {
        this._settings = this.getSettings();
        this._tokens = loadTokens(this.path);
        // Restore the Ubuntu panel home once for existing companion installs.
        // Later explicit movement choices remain available in the context menu.
        if (this._settings.get_int('panel-layout-version') < 1) {
            this._settings.set_string('mode', 'integrated');
            this._settings.set_string('desktop-motion', 'docked');
        }

        if (this._settings.get_int('panel-layout-version') < 2) {
            if (this._settings.get_int('workspace-width') === 640 &&
                this._settings.get_int('workspace-height') === 420) {
                this._settings.set_int('workspace-width', 520);
                this._settings.set_int('workspace-height', 300);
            }
            this._settings.set_int('panel-layout-version', 2);
        }

        if (this._settings.get_int('panel-layout-version') < 3) {
            if (this._settings.get_int('workspace-height') === 300)
                this._settings.set_int('workspace-height', 340);
            this._settings.set_int('panel-layout-version', 3);
        }
        this._client = new MiraClient();
        this._notch = new Notch(this._client, this._tokens, this._settings, {
            extensionPath: this.path,
            openPrefs: () => {
                try {
                    this.openPreferences();
                } catch (e) {
                    logError(e, 'mira: openPreferences');
                }
            },
        });

        // Top chrome, centered. trackFullscreen hides it when an app goes
        // fullscreen (the canonical global.display 'in-fullscreen-changed'
        // path) so a fullscreen game is never covered or minimized.
        Main.layoutManager.addTopChrome(this._notch, {trackFullscreen: true});

        this._wireClient();
        this._applyMode();

        // Build marker so it's obvious in the log which code is running.
        console.log('[mira] extension enabled — always-on eyes + glance build');

        this._settingsChangedId = this._settings.connect('changed', (_s, key) => {
            if (key === 'mode' || key === 'desktop-motion')
                this._applyMode();
            else if (key === 'routine-delivered')
                return;
            else if (key === 'mascot-preview')
                this._notch.previewMascot(this._settings.get_string('mascot-preview').split(':')[0]);
            else if (key === 'routine-preview')
                this._notch.showRoutine(this._settings.get_string('routine-preview').split(':')[0]);
            else if (key === 'greeting-preview')
                this._notch.showGreeting(false);
            else if (key === 'greeting-preview-birthday')
                this._notch.showGreeting(true);
            else
                this._notch.applySettings();
        });

        this._client.start();
        this._sessionChangedId = Main.sessionMode.connect('updated', () => this._syncSession());
        this._syncSession();
    }

    _syncSession() {
        if (!this._notch) return;
        this._notch.visible = !Main.sessionMode.isLocked;
        if (Main.sessionMode.isLocked) {
            this._notch.close();
            this._notch._desktop?.menu.close();
        }
    }

    _wireClient() {
        this._client.connect('media-changed', () => this._notch.refreshMedia());
        this._client.connect('power-changed', () => this._notch.refreshPower());
        this._client.connect('lyrics-changed', () => this._notch.onLyricsChanged());
        this._client.connect('lyrics-props-changed', () => this._notch.onLyricsChanged());
        this._client.connect('active-line-changed', idx => this._notch.onActiveLine(idx));
        this._client.connect('alert', (type, arg) => this._notch.showAlert(type, arg));
        this._client.connect('availability-changed', () => { this._notch.refreshMedia(); this._notch.refreshReminders(); });
        this._client.connect('pomodoro-changed', () => this._notch.refreshPomodoro());
        this._client.connect('water-changed', () => this._notch.refreshWater());
        this._client.connect('todo-changed', () => this._notch.refreshTodo());
        this._client.connect('shelf-changed', () => this._notch.refreshShelf());
        this._client.connect('reminders-changed', () => this._notch.refreshReminders());
        this._client.connect('reminder-due', (id, text) => this._notch.onReminderDue(id, text));
    }

    // Integrated mode hides the panel clock so the notch "replaces" it; floating
    // leaves the panel untouched. Both render the same centered pill.
    _applyMode() {
        const integrated = this._settings.get_string('mode') === 'integrated' &&
            this._settings.get_string('desktop-motion') === 'docked';
        const dateMenu = Main.panel?.statusArea?.dateMenu;
        if (dateMenu) {
            if (integrated && this._dateMenuWasVisible === undefined) {
                this._dateMenuWasVisible = dateMenu.visible;
                dateMenu.hide();
            } else if (!integrated && this._dateMenuWasVisible !== undefined) {
                if (this._dateMenuWasVisible)
                    dateMenu.show();
                this._dateMenuWasVisible = undefined;
            }
        }
        this._notch.applySettings();
    }

    disable() {
        if (this._sessionChangedId) {
            Main.sessionMode.disconnect(this._sessionChangedId);
            this._sessionChangedId = 0;
        }
        if (this._settingsChangedId) {
            this._settings.disconnect(this._settingsChangedId);
            this._settingsChangedId = 0;
        }
        // Restore the panel clock if we hid it.
        const dateMenu = Main.panel?.statusArea?.dateMenu;
        if (dateMenu && this._dateMenuWasVisible !== undefined) {
            if (this._dateMenuWasVisible)
                dateMenu.show();
            this._dateMenuWasVisible = undefined;
        }

        if (this._notch) {
            Main.layoutManager.removeChrome(this._notch);
            this._notch.destroy();
            this._notch = null;
        }
        if (this._client) {
            this._client.destroy();
            this._client = null;
        }
        this._settings = null;
        this._tokens = null;
    }
}
