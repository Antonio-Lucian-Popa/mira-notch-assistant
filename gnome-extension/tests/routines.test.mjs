import {readFile} from 'node:fs/promises';
import assert from 'node:assert/strict';
import test from 'node:test';
const source = await readFile(new URL('../lib/routines.js', import.meta.url), 'utf8');
const {validTime, routineEvents} = await import(`data:text/javascript,${encodeURIComponent(source)}`);
const config = {lunchEnabled: true, lunchTime: '13:00', workEndEnabled: true, workEndTime: '18:00', leadMinutes: 5, weekdaysOnly: true};
const at = (h,m) => new Date(2026,9,27,h,m,0);
test('validates hours including midnight and minute zero', () => {
    assert.equal(validTime('0:00'), '00:00'); assert.equal(validTime('18:00'), '18:00');
    for (const bad of ['24:00','18:60','abc','12:1','-1:00']) assert.equal(validTime(bad),null);
});
test('lunch, heads up and goodbye each have their own deadline', () => {
    assert.deepEqual(routineEvents(config,at(12,59)), []);
    assert.equal(routineEvents(config,at(13,0))[0].type, 'lunch');
    assert.equal(routineEvents(config,at(17,55))[0].type, 'work-soon');
    assert.deepEqual(routineEvents(config,at(18,0)).map(e=>e.type), ['work-end']);
});
test('delivery markers persist and late starts do not replay old greetings', () => {
    const first = routineEvents(config,at(13,2))[0];
    assert.deepEqual(routineEvents(config,at(13,3),{lunch:first.key}),[]);
    assert.deepEqual(routineEvents(config,at(16,0)),[]);
    const end = routineEvents(config,at(18,0))[0];
    assert.deepEqual(routineEvents(config,at(18,1),{'work-end':end.key}),[]);
});
test('weekday toggle and midnight lead respect the target workday', () => {
    const saturday = new Date(2026,9,31,13,0);
    assert.deepEqual(routineEvents(config,saturday),[]);
    assert.equal(routineEvents({...config,weekdaysOnly:false},saturday)[0].type,'lunch');
    const sunday = new Date(2026,10,1,23,58);
    assert.equal(routineEvents({...config,workEndTime:'00:03'},sunday)[0].type,'work-soon');
});
test('disabled schedules and zero lead do not emit a heads up', () => {
    assert.deepEqual(routineEvents({...config,lunchEnabled:false},at(13,0)),[]);
    assert.deepEqual(routineEvents({...config,leadMinutes:0},at(17,55)),[]);
});

test('short suspend across midnight catches the previous day once', () => {
    const events = routineEvents({...config,workEndTime:'23:59',weekdaysOnly:false},new Date(2026,9,28,0,5));
    assert.deepEqual(events.map(e=>e.type),['work-end']);
    assert.match(events[0].key,/2026-10-27/);
});

test('daily summary is independent of weekday-only work routines',()=>{
 const events=routineEvents({...config,summaryEnabled:true,summaryTime:'21:00'},new Date(2026,9,31,21,0));
 assert.deepEqual(events.map(e=>e.type),['summary']);
 assert.deepEqual(routineEvents({...config,summaryEnabled:true,summaryTime:'21:00'},new Date(2026,9,31,21,0),{summary:events[0].key}),[]);
});
