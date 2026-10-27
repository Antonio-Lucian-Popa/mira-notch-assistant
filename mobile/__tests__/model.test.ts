import { duration, reminderDraft, urgency } from '../src/model';
const now = new Date(2026, 9, 27, 14, 0, 0).getTime();
it('schedules relative and next-day clock reminders in local time', () => {
  expect(reminderDraft('  Prânz ', '20', '', false, now)).toEqual({
    title: 'Prânz',
    due: now + 1200000,
    daily: false,
  });
  const tomorrow = new Date(2026, 9, 28, 13, 0, 0).getTime();
  expect(reminderDraft('Prânz', '', '13:00', true, now).due).toBe(tomorrow);
});
it('rejects empty, invalid, negative and out-of-range drafts', () => {
  for (const args of [
    ['', '20', ''],
    ['A', '-1', ''],
    ['A', '0', ''],
    ['A', '1.5', ''],
    ['A', '20', '24:00'],
    ['A', '20', '12:70'],
  ]) {
    expect(() =>
      reminderDraft(args[0], args[1], args[2], false, now),
    ).toThrow();
  }
});
it('uses the earliest due occurrence without mutating storage order', () => {
  const later = { id: '2', title: 'Later', due: now + 30000, daily: false };
  const first = { id: '1', title: 'First', due: now - 1000, daily: false };
  const list = [later, first];
  expect(urgency(list, now)?.id).toBe('1');
  expect(list[0].id).toBe('2');
  expect(urgency([later], now - 60000)).toBeUndefined();
});
it('formats remaining time without negative values', () => {
  expect(duration(-1)).toBe('0:00');
  expect(duration(1)).toBe('0:01');
  expect(duration(25000)).toBe('0:25');
  expect(duration(3660000)).toBe('1h 1m');
});
