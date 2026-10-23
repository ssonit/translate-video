import assert from "node:assert/strict";
import test from "node:test";
import { chipLabel, firstGap, lockNote, markFor, summarize, type ToolStatus } from "./setupStatus.ts";

const ok = (id: ToolStatus["id"]): ToolStatus => ({ id, ready: true, detail: "" });
const gap = (id: ToolStatus["id"]): ToolStatus => ({ id, ready: false, detail: "" });
// Thứ tự giống `run_checks` bên Rust.
const everything = (): ToolStatus[] => [
  ok("ffmpeg"),
  ok("ollama"),
  ok("model"),
  ok("voice"),
  ok("piper"),
  ok("whisper"),
  ok("ytdlp"),
];
const without = (...ids: ToolStatus["id"][]): ToolStatus[] =>
  everything().map((tool) => (ids.includes(tool.id) ? gap(tool.id) : tool));

test("before the first result the chip says it is checking and nothing is locked", () => {
  const summary = summarize([], true);
  assert.equal(summary.kind, "checking");
  assert.equal(chipLabel(summary), "Đang kiểm tra");
  assert.equal(lockNote(summary), "");
});

test("no result and no check running means the check could not be read, and nothing is locked", () => {
  const summary = summarize([], false);
  assert.equal(summary.kind, "failed");
  assert.equal(chipLabel(summary), "Chưa kiểm tra được");
  assert.equal(lockNote(summary), "");
});

test("every required tool present is ready, even when the optional ones are missing", () => {
  const summary = summarize(without("ytdlp", "piper"), false);
  assert.equal(summary.kind, "ready");
  assert.equal(chipLabel(summary), "Đủ đồ nghề");
  assert.equal(lockNote(summary), "");
});

test("one missing required tool is named in the chip and in the lock note", () => {
  const summary = summarize(without("whisper"), false);
  assert.equal(summary.kind, "gap");
  assert.deepEqual(summary.missing, ["whisper"]);
  assert.equal(chipLabel(summary), "Thiếu Whisper, xem");
  assert.equal(lockNote(summary), "Chưa chạy được: thiếu Whisper.");
});

test("several missing tools are counted in the chip and listed in the lock note", () => {
  const summary = summarize(without("ffmpeg", "whisper"), false);
  assert.equal(chipLabel(summary), "Thiếu 2 mục, xem");
  assert.equal(lockNote(summary), "Chưa chạy được: thiếu FFmpeg, Whisper.");
});

test("a re-check keeps showing the last result instead of flipping back to checking", () => {
  assert.equal(summarize(without("whisper"), true).kind, "gap");
  assert.equal(summarize(everything(), true).kind, "ready");
});

test("firstGap picks the first missing required tool and ignores the optional ones", () => {
  assert.equal(firstGap(everything()), undefined);
  assert.equal(firstGap(without("ytdlp", "piper")), undefined);
  assert.equal(firstGap(without("voice")), "voice");
  assert.equal(firstGap(without("whisper", "model")), "model");
});

test("a row without a result is neutral while checking and is never called missing", () => {
  assert.deepEqual(markFor(undefined, false, true), { text: "Đang kiểm tra", tone: "wait" });
  assert.deepEqual(markFor(undefined, true, true), { text: "Đang kiểm tra", tone: "wait" });
  assert.deepEqual(markFor(undefined, false, false), { text: "Chưa rõ", tone: "wait" });
});

test("a row with a result says present, missing or optional", () => {
  assert.deepEqual(markFor(ok("ffmpeg"), false, false), { text: "Có", tone: "ready" });
  assert.deepEqual(markFor(gap("whisper"), false, false), { text: "Thiếu", tone: "bad" });
  assert.deepEqual(markFor(gap("ytdlp"), true, false), { text: "Tùy chọn", tone: "bad" });
});
