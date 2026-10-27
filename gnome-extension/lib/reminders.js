// Pure time calculations. Recompute from wall time so suspend/resume and clock
// changes cannot accumulate drift; persistence and scheduling live in the daemon.
export function countdown(seconds) {
    const s = Math.max(0, Math.ceil(seconds));
    const two = value => String(value).padStart(2, '0');
    return s >= 3600 ? `${Math.floor(s / 3600)}:${two(Math.floor(s / 60) % 60)}:${two(s % 60)}`
        : `${two(Math.floor(s / 60))}:${two(s % 60)}`;
}

export function reminderState(reminder, now = Math.floor(Date.now() / 1000)) {
    const remaining = Math.max(0, reminder.due - now);
    const urgency = remaining === 0 ? 'due' : remaining <= 60 ? 'urgent' : remaining <= 300 ? 'soon' : 'calm';
    return {remaining, urgency, countdown: countdown(remaining),
        fraction: Math.max(0, Math.min(1, remaining / Math.max(1, reminder.due - reminder.created)))};
}

// "15m", "2h", "30s" or a local wall-clock time, e.g. "14:30".
export function parseWhen(input, now = new Date()) {
    const value = input.trim().toLowerCase();
    const relative = /^(\d+)\s*(s|m|h)?$/.exec(value);
    if (relative) {
        const seconds = Number(relative[1]) * ({s: 1, m: 60, h: 3600}[relative[2] ?? 'm']);
        if (seconds < 1 || seconds > 366 * 86400)
            return null;
        return Math.floor(now.getTime() / 1000) + seconds;
    }
    const clock = /^(\d{1,2}):(\d{2})$/.exec(value);
    if (!clock || Number(clock[1]) > 23 || Number(clock[2]) > 59)
        return null;
    const date = new Date(now);
    date.setHours(Number(clock[1]), Number(clock[2]), 0, 0);
    if (date <= now)
        date.setDate(date.getDate() + 1);
    return Math.floor(date.getTime() / 1000);
}
