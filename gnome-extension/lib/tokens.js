// Loads design tokens from `tokens.json` (copied from design/tokens.json at
// build time). Falls back to built-in defaults so the extension still runs if
// the file is missing — the defaults mirror design/tokens.json.

import GLib from 'gi://GLib';

const DEFAULTS = {
    notch: {
        closed: {width: 180, height: 32, radiusBottom: 16},
        hover: {width: 600, height: 196, radiusBottom: 34},
        alert: {width: 360, height: 60, radiusBottom: 24},
        greeting: {width: 440, height: 72, radiusBottom: 28},
        minimal: {width: 120, height: 6, radiusBottom: 4},
        contentPadding: 14,
        hoverCloseDelayMs: 300,
    },
    colors: {
        background: '#0A0A0Bf2',
        text: '#F2F2F3',
        textDim: '#9A9AA0',
        accent: '#5B8CFF',
        ring: '#2EE6A6',
        focus: '#FF5A5A',
        water: '#3B9CFF',
        waterDim: '#1C4E86',
        todoDone: '#2EE67A',
    },
    motion: {
        open: {durationMs: 420},
        close: {durationMs: 260},
        alertIn: {durationMs: 320},
        alertOut: {durationMs: 220},
        pageSwap: {durationMs: 240},
        lyricLine: {durationMs: 280},
        alertHoldMs: 2600,
    },
    pages: ['music', 'lyrics', 'focus', 'water', 'shelf', 'todo'],
    sizing: {
        wideMonitorMinWidthPx: 2560,
        wideScaleFactor: 1.25,
        minScale: 0.8,
        maxScale: 1.6,
    },
    visualizer: {bars: 5, minHeight: 3, maxHeight: 14, gap: 2, fps: 30},
    mascot: {idleTimeoutMs: 60000, sleepTransitionMs: 1200},
};

export function loadTokens(extensionPath) {
    try {
        const path = GLib.build_filenamev([extensionPath, 'tokens.json']);
        const [ok, bytes] = GLib.file_get_contents(path);
        if (ok) {
            const text = new TextDecoder().decode(bytes);
            return mergeDefaults(JSON.parse(text));
        }
    } catch (_e) {
        // Fall through to defaults.
    }
    return DEFAULTS;
}

// Shallow-merge parsed tokens over defaults so a partial file still works.
// (GJS has no structuredClone, so deep-clone the defaults via JSON.)
function mergeDefaults(parsed) {
    const out = JSON.parse(JSON.stringify(DEFAULTS));
    for (const key of Object.keys(parsed))
        out[key] = parsed[key];
    return out;
}
