import assert from 'node:assert/strict';
import test from 'node:test';

import { overlayOutputLimitArgs, posterTimeForRange } from './render-options.mjs';

test('keeps the preferred poster timestamp when it is within the rendered range', () => {
  assert.equal(posterTimeForRange(40.5, 0, 170), 40.5);
});

test('moves the poster timestamp into a partial rendered range', () => {
  assert.equal(posterTimeForRange(40.5, 44, 62), 53);
});

test('uses a midpoint when the preferred timestamp is at the exclusive end', () => {
  assert.equal(posterTimeForRange(62, 44, 62), 53);
});

test('rejects an empty or invalid render range', () => {
  assert.throws(() => posterTimeForRange(40.5, 62, 44), RangeError);
  assert.throws(() => posterTimeForRange(40.5, 0, Number.NaN), RangeError);
});

test('caps the overlay pass to the number of frames rendered for the selected range', () => {
  assert.deepEqual(overlayOutputLimitArgs(540), ['-frames:v', '540']);
  assert.throws(() => overlayOutputLimitArgs(0), RangeError);
});
