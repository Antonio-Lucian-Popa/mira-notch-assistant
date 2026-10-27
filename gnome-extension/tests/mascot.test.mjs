import {readFile} from 'node:fs/promises';
import assert from 'node:assert/strict';
import test from 'node:test';
const source=await readFile(new URL('../lib/mascot.js',import.meta.url),'utf8');
const {contextMood,THEMES,EXPRESSIONS,looksNatural,summaryText}=await import(`data:text/javascript,${encodeURIComponent(source)}`);
test('urgency wins over music/focus/evening and contextual preference',()=>{
    assert.equal(contextMood({urgency:'due',focus:true,playing:true,hour:23,enabled:false}),'reminder');
    assert.equal(contextMood({focus:true,playing:true,hour:23}),'focus');
    assert.equal(contextMood({playing:true,hour:23}),'music');
    assert.equal(contextMood({hour:23}),'evening');
    assert.equal(contextMood({hour:12}),'idle');
    assert.equal(contextMood({focus:true,enabled:false}),'idle');
});
test('four native themes and every studio expression have stable names',()=>{
    assert.deepEqual(Object.keys(THEMES),['classic','robot','cat','pixel']);
    assert.ok(EXPRESSIONS.includes('confetti'));assert.ok(EXPRESSIONS.includes('wink'));
});
test('natural intent detection keeps normal titles separate',()=>{
    for(const text of ['Sună-l pe Alex peste 20 de minute','Call in one hour','Raport mâine la 10','Ședință în fiecare luni la 9'])assert.ok(looksNatural(text),text);
    for(const text of ['Call Alex','Meeting in office','Plan at home'])assert.equal(looksNatural(text),false,text);
});
test('summary text displays real numbers without a score',()=>{
    assert.equal(summaryText({focus_seconds:185,focus_sessions:1,todos_completed:2,water_glasses:3,reminders_completed:4}),'3 min focus · 1 sesiuni · 2 sarcini · 3 pahare · 4 remindere');
});
