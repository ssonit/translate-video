import assert from "node:assert/strict";
import test from "node:test";
import { fitWarning, lineWarning, nextFitWarning } from "./fitWarning.ts";

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

test("empty translated speech is reviewable in video, audio and caption modes", () => {
  const empty = { ...line, translation: " \n " };
  assert.equal(lineWarning(empty), "Chưa có lời cho câu này");
  assert.equal(lineWarning(empty, false, true), "Chưa có lời cho câu này");
  assert.equal(lineWarning(empty, true), "Chưa có lời cho câu này");
  assert.equal(lineWarning({ ...empty, source: " " }), "");
});

test("missing line audio is reported as a missing file, not silent output", () => {
  const missing = { ...line, missing_audio: true };
  assert.equal(lineWarning(missing), "Thiếu file tiếng của câu");
  assert.equal(lineWarning(missing, false, true), "Thiếu file tiếng của câu");
  assert.equal(lineWarning(missing, true), "");
  assert.equal(lineWarning({ ...missing, translation: "" }), "Chưa có lời cho câu này");
});

test("combines independent warnings and hides only fit warnings for audio", () => {
  const missing = { ...line, missing_audio: true, fit: { cut_seconds: 1, speed: 1.4 } };
  assert.equal(lineWarning(missing), "Thiếu file tiếng của câu · Bị cắt 1 giây cuối · Đã tăng tốc 1,4×");
  assert.equal(lineWarning(missing, false, true), "Thiếu file tiếng của câu");
  assert.equal(lineWarning(missing, true), "");
  assert.equal(lineWarning(line), "");
});

test("caption review joins the existing navigation and stays hidden for audio", () => {
  const captioned = { ...line, captions: ["overlap", "short_duration", "dense_text", "overlap"] as const };
  const reviewed = { ...captioned, captions: [...captioned.captions] };
  const warning = "Mốc câu phụ đề chồng nhau · Phụ đề hiện dưới 0,65 giây, nên xem lại · Phụ đề nhiều chữ so với thời gian, nên xem lại";
  assert.equal(lineWarning(reviewed), warning);
  assert.equal(lineWarning(reviewed, true), warning);
  assert.equal(lineWarning(reviewed, false, true), "");
  assert.equal(lineWarning({ ...line, captions: ["invalid_timing"] }), "Mốc thời gian phụ đề không hợp lệ");
  assert.equal(nextFitWarning([0, 2], 0), 2);
});
