import assert from "node:assert/strict";
import test from "node:test";
import { fitWarning, nextFitWarning } from "./fitWarning.ts";

const line = { start: 0, end: 1, source: "Hi", translation: "Chào" };

test("old lines have no warning and old cuts do not invent a speed", () => {
  assert.equal(fitWarning(line), "");
  assert.equal(fitWarning({ ...line, fit: { cut_seconds: 1.25, speed: null } }), "Bị cắt 1,25 giây cuối");
});

test("shows precise speed and cut separately, ignoring invalid numbers", () => {
  assert.equal(fitWarning({ ...line, fit: { cut_seconds: 0, speed: 1.08 } }), "Đã tăng tốc 1,08×");
  assert.equal(fitWarning({ ...line, fit: { cut_seconds: 0, speed: 1.004 } }), "Đã tăng tốc 1,004×");
  assert.equal(fitWarning({ ...line, fit: { cut_seconds: 2, speed: 1.08 } }), "Bị cắt 2 giây cuối · Đã tăng tốc 1,08×");
  assert.equal(fitWarning({ ...line, fit: { cut_seconds: NaN, speed: Infinity } }), "");
});

test("review cycles through original line indexes without changing edit indexes", () => {
  assert.equal(nextFitWarning([2, 7], -1), 2);
  assert.equal(nextFitWarning([2, 7], 2), 7);
  assert.equal(nextFitWarning([2, 7], 7), 2);
  assert.equal(nextFitWarning([], -1), undefined);
});
