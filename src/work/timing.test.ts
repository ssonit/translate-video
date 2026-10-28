import assert from "node:assert/strict";
import test from "node:test";
import { cueTimes, timingProblem, movedTimes, loopRange } from "./timing.ts";
import { pickedEdits } from "./format.ts";
import type { LinePick } from "./types.ts";

const lines = [
  { start: 0, end: 1, source: "a", translation: "Một", revision: "v1" },
  { start: 2, end: 3, source: "b", translation: "Hai", revision: "v1" },
  { start: 4, end: 5, source: "c", translation: "Ba", revision: "v1" },
];

test("a timing-only draft reaches retake without requesting a new voice", () => {
  const picks = { 1: { text: "Hai", again: false, start: 1.8, end: 3.2, revision: "v1" } };
  assert.deepEqual(pickedEdits(lines, picks), [{ index: 1, text: "Hai", again: false, revision: "v1", start: 1.8, end: 3.2 }]);
  assert.deepEqual(cueTimes(lines[1], picks[1]), { start: 1.8, end: 3.2 });
  assert.equal(timingProblem(lines, picks, 1, 1.8, 3.2, 6), "");
});

test("text edits and forced voice retakes remain independent of pending timing", () => {
  const pick: LinePick = { text: "Hai sửa", again: false, start: 1.8, end: 3.2, revision: "v1" };
  assert.equal(pickedEdits(lines, { 1: pick })[0].again, true);
  assert.equal(pickedEdits(lines, { 1: { text: "Hai", again: true } })[0].again, true);
  assert.deepEqual(pickedEdits(lines, { 1: { text: "Hai", again: false, start: 2, end: 3 } }), []);
});

test("invalid ranges and new overlap are rejected; preexisting overlap may shrink", () => {
  for (const [start, end] of [[NaN, 3], [2, Infinity], [-1, 3], [2, 2.01], [2, 7], [0.5, 3], [2, 4.5]]) {
    assert.notEqual(timingProblem(lines, {}, 1, start, end, 6), "");
  }
  const overlapping = [{ ...lines[0], end: 2.5 }, lines[1], lines[2]];
  assert.equal(timingProblem(overlapping, {}, 1, 2.2, 3, 6), "");
  assert.notEqual(timingProblem(overlapping, {}, 1, 1.5, 3, 6), "");
  assert.notEqual(timingProblem(lines, { 0: { text: "Một", again: false, end: 1.9 } }, 1, 1.8, 3, 6), "");
});

test("dragging clamps the whole interval at video edges and keeps milliseconds", () => {
  assert.deepEqual(movedTimes(1, 2, -3, "move", 5), { start: 0, end: 1 });
  assert.deepEqual(movedTimes(1, 2, 10, "move", 5), { start: 4, end: 5 });
  assert.deepEqual(movedTimes(1, 2, 10, "start", 5), { start: 1.92, end: 2 });
  assert.deepEqual(movedTimes(1, 2, -10, "end", 5), { start: 1, end: 1.08 });
  assert.deepEqual(movedTimes(1, 2, .12345, "move", 5), { start: 1.123, end: 2.123 });
  assert.deepEqual(loopRange(.1, 1.9, 2), { start: 0, end: 2 });
});
