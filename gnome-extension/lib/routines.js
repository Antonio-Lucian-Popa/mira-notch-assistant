// Local calendar times (not 24-hour intervals), including midnight and DST.
export function validTime(text) {
    const match = /^(\d{1,2}):(\d{2})$/.exec(text.trim());
    return match && +match[1] < 24 && +match[2] < 60
        ? `${match[1].padStart(2, '0')}:${match[2]}` : null;
}

export function routineEvents(config, now, delivered = {}) {
    const result = [];
    for (const offset of [-1, 0, 1]) {
        const day = new Date(now);
        day.setDate(day.getDate() + offset);
        if (config.weekdaysOnly && [0, 6].includes(day.getDay()))
            continue;
        const date = `${day.getFullYear()}-${day.getMonth() + 1}-${day.getDate()}`;
        const events = [];
        for (const [enabled, time, type] of [[config.lunchEnabled, config.lunchTime, 'lunch'], [config.workEndEnabled, config.workEndTime, 'work-end']]) {
            const clock = validTime(time ?? '');
            if (!enabled || !clock)
                continue;
            const [hh, mm] = clock.split(':').map(Number);
            const target = new Date(day);
            target.setHours(hh, mm, 0, 0);
            events.push({type, time: target.getTime(), clock});
            if (type === 'work-end' && config.leadMinutes > 0)
                events.push({type: 'work-soon', time: target.getTime() - config.leadMinutes * 60000, endTime: target.getTime(), clock});
        }
        for (const event of events) {
            if (event.type === 'work-soon' && now.getTime() >= event.endTime) continue;
            const age = now.getTime() - event.time;
            const key = `${date}:${event.clock}`;
            // Short catch-up window covers suspend/resume without replaying
            // yesterday's lunch when starting the extension late at night.
            if (age >= 0 && age <= 15 * 60000 && delivered[event.type] !== key)
                result.push({...event, key});
        }
    }
    // Skip a stale "nearly done" if we are already at the goodbye time.
    return result.filter(event => event.type !== 'work-soon' || !result.some(other => other.type === 'work-end'));
}
