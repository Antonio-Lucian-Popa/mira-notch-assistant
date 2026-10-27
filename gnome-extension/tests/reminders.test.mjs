import {readFile} from 'node:fs/promises';
import assert from 'node:assert/strict';
import test from 'node:test';
const source = await readFile(new URL('../lib/reminders.js', import.meta.url), 'utf8');
const {countdown, parseWhen, reminderState} = await import(`data:text/javascript,${encodeURIComponent(source)}`);

test('urgency progresses at exact 5-minute, 1-minute and due boundaries', () => {
    const reminder = {due: 1000, created: 0};
    for (const [now, urgency] of [[699, 'calm'], [700, 'soon'], [939, 'soon'], [940, 'urgent'], [999, 'urgent'], [1000, 'due'], [2000, 'due']])
        assert.equal(reminderState(reminder, now).urgency, urgency);
    assert.equal(reminderState(reminder, 2000).countdown, '00:00');
    assert.equal(reminderState(reminder, -1).fraction, 1);
    assert.equal(reminderState(reminder, 500).fraction, 0.5);
});
test('relative deadlines support seconds, minutes and hours', () => {
    const now = new Date(2026, 9, 27, 12, 0, 0);
    const epoch = now.getTime() / 1000;
    assert.equal(parseWhen('15m', now), epoch + 900);
    assert.equal(parseWhen('2h', now), epoch + 7200);
    assert.equal(parseWhen('30s', now), epoch + 30);
    assert.equal(parseWhen('5', now), epoch + 300);
    for (const bad of ['', '0m', '-1h', '999999999h', '25:00', '12:60', 'later', '1.5m'])
        assert.equal(parseWhen(bad, now), null);
});
test('clock times use local time and past times roll to tomorrow', () => {
    const now = new Date(2026, 9, 27, 18, 0, 0);
    assert.equal(parseWhen('19:15', now), new Date(2026, 9, 27, 19, 15, 0).getTime() / 1000);
    assert.equal(parseWhen('09:30', now), new Date(2026, 9, 28, 9, 30, 0).getTime() / 1000);
});
test('countdown recomputes after resume instead of accumulating timer drift', () => {
    const r = {due: 9000, created: 1000};
    assert.equal(reminderState(r, 1000).remaining, 8000);
    assert.equal(reminderState(r, 9001).remaining, 0);
    assert.equal(countdown(3661), '1:01:01');
    assert.equal(countdown(-1), '00:00');
});

const mascotSource = await readFile(new URL('../lib/mascot.js', import.meta.url), 'utf8');
const {THEMES} = await import(`data:text/javascript,${encodeURIComponent(mascotSource)}`);
const notch = await readFile(new URL('../lib/notch.js', import.meta.url), 'utf8');
const methods = notch.slice(notch.indexOf('    onReminderDue(id, text) {'), notch.indexOf('    _buildSummaryPage() {'));
function fixture() {
    let notices = 0;
    const h = new Function('Main', 'reminderState', 'Clutter', 'THEMES', `return ({${methods.replace(/\n    }\n/g, '\n    },\n')}});`)(
        {notify: () => notices++}, reminderState, {AnimationMode: {EASE_OUT_QUAD: 1, EASE_OUT_BACK: 2}}, THEMES);
    Object.assign(h, {
        _reminderNotified: new Set(), _reminders: [], _reminderRows: [], _reminderAlive: true,
        _settings: {get_string: () => 'classic', get_double: () => 0.6}, _reminderTime: {}, _reminderName: {}, _reminderBar: {set() {}},
        _reminderBadge: {}, _reminderFeedback: {}, _eyes: {_eyes: [{set_style() {}}, {set_style() {}}]},
    });
    return {h, notices: () => notices};
}
test('overdue reminder notifies once and remains visible until acknowledged', () => {
    const {h, notices} = fixture();
    const now = Math.floor(Date.now() / 1000);
    const reminder = {id: 1, text: 'Call Mom', due: now - 1, created: now - 100};
    h._reminders = [reminder];
    h._reminderRows = [{reminder, row: {}, time: {}}];
    h._tickReminders(false); h._tickReminders(false); h.onReminderDue(1, reminder.text);
    assert.equal(notices(), 1);
    assert.equal(h._reminderTime.text, 'Acum');
    assert.equal(h._reminders.length, 1);
    assert.equal(h._reminderUrgency, 'due');
});
test('clearing reminders resets the mascot and countdown', () => {
    const {h} = fixture();
    h._tickReminders(false);
    assert.equal(h._reminderUrgency, 'calm');
    assert.equal(h._reminderTime.text, '—');
    assert.equal(h._reminderBadge.label, '');
});
