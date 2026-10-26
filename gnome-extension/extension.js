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

        this._client = new MiraClient();
        this._notch = new Notch(this._client, this._tokens, this._settings);

        // Top chrome, centered. trackFullscreen hides it when an app goes
        // fullscreen (the canonical global.display 'in-fullscreen-changed'
        // path) so a fullscreen game is never covered or minimized.
        Main.layoutManager.addTopChrome(this._notch, {trackFullscreen: true});

        this._wireClient();
        this._applyMode();

        this._settingsChangedId = this._settings.connect('changed', (_s, key) => {
            if (key === 'mode')
                this._applyMode();
            else
                this._notch.applySettings();
        });

        this._client.start();
    }

    _wireClient() {
        this._client.connect('media-changed', () => this._notch.refreshMedia());
        this._client.connect('power-changed', () => this._notch.refreshPower());
        this._client.connect('lyrics-changed', () => this._notch.onLyricsChanged());
        this._client.connect('lyrics-props-changed', () => this._notch.onLyricsChanged());
        this._client.connect('active-line-changed', idx => this._notch.onActiveLine(idx));
        this._client.connect('alert', (type, pct) => this._notch.showAlert(type, pct));
        this._client.connect('availability-changed', () => this._notch.refreshMedia());
    }

    // Integrated mode hides the panel clock so the notch "replaces" it; floating
    // leaves the panel untouched. Both render the same centered pill.
    _applyMode() {
        const integrated = this._settings.get_string('mode') === 'integrated';
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
