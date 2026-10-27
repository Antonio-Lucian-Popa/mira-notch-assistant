import {readFile} from 'node:fs/promises';
import assert from 'node:assert/strict';
import test from 'node:test';
const source = await readFile(new URL('../lib/gaze.js', import.meta.url), 'utf8');
const {gazeOffset} = await import(`data:text/javascript,${encodeURIComponent(source)}`);

test('centered pointer leaves the eyes centered', () => {
    assert.deepEqual(gazeOffset(0, 0), {x: 0, y: 0});
});
test('gaze points into every quadrant, including other monitors', () => {
    for (const dx of [-2500, -200, 200, 2500]) {
        for (const dy of [-1400, -100, 100, 1400]) {
            const {x, y} = gazeOffset(dx, dy);
            assert.equal(Math.sign(x), Math.sign(dx));
            assert.equal(Math.sign(y), Math.sign(dy));
            assert.ok((x / 8) ** 2 + (y / 5) ** 2 <= 1.000001);
        }
    }
});
test('gaze keeps changing direction beyond the old saturation point', () => {
    assert.ok(gazeOffset(1200, 400).x > gazeOffset(1200, 1400).x);
    assert.ok(gazeOffset(1200, 400).y < gazeOffset(1200, 1400).y);
});
test('movement near the notch is continuous', () => {
    assert.ok(gazeOffset(300, 0).x > gazeOffset(100, 0).x);
    assert.equal(gazeOffset(1, 0).y, 0);
    assert.ok(gazeOffset(1, 0).x < 0.02);
});

const notchSource = await readFile(new URL('../lib/notch.js', import.meta.url), 'utf8');
const methods = notchSource.slice(notchSource.indexOf('    _startEyeFollow() {'), notchSource.indexOf('    // The animation library.'));
function followHarness() {
    let removed = 0;
    let watches = 0;
    let callback;
    const watcher = {getPointerWatcher: () => ({addWatch: (_interval, fn) => {
        watches++; callback = fn;
        return {remove: () => removed++};
    }})};
    const fixture = new Function('PointerWatcher', 'global', 'gazeOffset', 'Clutter', `return ({${methods.replace(/\n    }\n/g, '\n    },\n')}});`)(watcher,
        {get_pointer: () => [900, 500]}, gazeOffset, {AnimationMode: {EASE_OUT_QUAD: 1}});
    let eased;
    Object.assign(fixture, {
        mapped: true, _open: false,
        _eyes: {visible: true, ease: target => {eased = target;}, remove_all_transitions() {}},
        get_transformed_position: () => [800, 0], get_transformed_size: () => [180, 32],
    });
    return {fixture, motion: (x, y) => callback(x, y), counts: () => ({removed, watches}), target: () => eased};
}
test('pointer watch is registered once and removed when stopped', () => {
    const h = followHarness();
    h.fixture._startEyeFollow(); h.fixture._startEyeFollow();
    assert.equal(h.counts().watches, 1);
    h.motion(-1000, 500);
    assert.ok(h.target().translation_x < 0);
    h.fixture._stopEyeFollow(); h.fixture._stopEyeFollow();
    assert.equal(h.counts().removed, 1);
    assert.equal(h.fixture._eyes.translation_x, 0);
    assert.equal(h.fixture._eyes.translation_y, 0);
});
test('open or hidden mascot does not animate on mouse movement', () => {
    const h = followHarness();
    h.fixture._startEyeFollow();
    const previous = h.target();
    h.fixture._open = true; h.motion(-300, 20);
    assert.equal(h.target(), previous);
    h.fixture._open = false; h.fixture._eyes.visible = false; h.motion(-300, 20);
    assert.equal(h.target(), previous);
});
