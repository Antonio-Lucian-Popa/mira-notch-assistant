// Native/vector appearances; no downloaded sprites or image assets.
export const THEMES = {
    classic: {label: 'Clasic', color: '#8FB0FF', width: 9, height: 9},
    robot: {label: 'Roboțel', color: '#76F5BF', width: 12, height: 8},
    cat: {label: 'Pisică', color: '#FFCF77', width: 12, height: 13},
    pixel: {label: 'Pixel', color: '#C5A0FF', width: 10, height: 10},
};
export const EXPRESSIONS = ['blink','doubleBlink','glanceL','glanceR','lookAround','lookUp','squintHappy','wink','surprised','excited','sleepy','dizzy','love','hearts','rainbow','confetti'];
export function contextMood({urgency='calm', focus=false, playing=false, hour=12, enabled=true}) {
    if (urgency !== 'calm') return 'reminder';
    if (!enabled) return 'idle';
    if (focus) return 'focus';
    if (playing) return 'music';
    if (hour >= 21 || hour < 6) return 'evening';
    return 'idle';
}
export function looksNatural(text) {
    return /\s(?:peste\s+(?:\d+|o|un)\b|in\s+(?:\d+|one)\b|m[âa]ine\b|tomorrow\b|every\b|(?:[îi]n\s+)?fiecare\b|(?:azi|today)\b|(?:la|at)\s+\d)/i.test(text);
}
export function summaryText(stats) {
    return `${Math.floor(stats.focus_seconds / 60)} min focus · ${stats.focus_sessions} sesiuni · ${stats.todos_completed} sarcini · ${stats.water_glasses} pahare · ${stats.reminders_completed} remindere`;
}
