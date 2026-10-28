import assert from "node:assert/strict";
import test from "node:test";
import { againPlan, draftProgress, freshSteps, showAsTextOnly, videoToSave, watchHeading, overallEstimate, workflowPhases } from "./format.ts";
import type { DraftBrief } from "./types.ts";

test("text only skips the voice steps", () => {
  const steps = freshSteps(false, true);
  assert.equal(steps.find((step) => step.id === "speak")?.state, "skipped");
  assert.equal(steps.find((step) => step.id === "fit")?.state, "skipped");
  assert.equal(steps.find((step) => step.id === "merge")?.label, "Ghi chữ");
  const dubbed = freshSteps(false);
  assert.equal(dubbed.find((step) => step.id === "speak")?.state, "pending");
  assert.equal(dubbed.find((step) => step.id === "merge")?.label, "Ghép video");
});

test("waiting for names freezes processing time and hides ETA until resumed", () => {
  const steps = freshSteps().map((step) => ({ ...step, state: step.id === "extract" ? "done" : "pending" }));
  const paused = { pausedMs: 5000, sinceMs: 20000 };
  const before = overallEstimate(steps, 1000, 30000, paused);
  const later = overallEstimate(steps, 1000, 90000, paused);
  assert.equal(before.elapsed, 14);
  assert.deepEqual(before, later);
  assert.equal(later.remaining, null);
  const resumed = overallEstimate(steps, 1000, 100000, { pausedMs: 75000, sinceMs: null });
  assert.equal(resumed.elapsed, 24);
  assert.ok(resumed.remaining! > 0);
  assert.equal(overallEstimate(steps, null, 100000, paused).elapsed, 0);
});

test("four phases retain pending work, errors and the explicit name wait", () => {
  const steps = freshSteps(false, true).map((step) => ({ ...step, state: ["extract", "transcribe", "split"].includes(step.id) ? "done" : step.state }));
  const phases = workflowPhases(steps, true);
  assert.deepEqual(phases.map((phase) => phase.state), ["done", "waiting", "pending", "pending"]);
  const failed = steps.map((step) => step.id === "translate" ? { ...step, state: "error" } : step);
  assert.equal(workflowPhases(failed)[2].state, "error");
  assert.equal(workflowPhases(freshSteps().map((step) => ({ ...step, state: "skipped" })))[3].state, "done");
});

test("a text-only result keeps its own heading before the library reloads", () => {
  assert.equal(showAsTextOnly("D:\\tap.vi.chu.mp4", "D:\\tap.vi.mp4", true), true);
  assert.equal(watchHeading("D:\\tap.vi.chu.mp4", "D:\\tap.vi.mp4", true), "Video chỉ có chữ");
  assert.equal(showAsTextOnly("D:\\tap.vi.chu.mp4", "D:\\tap.vi.mp4", false), false);
  assert.equal(watchHeading("D:\\tap.vi.chu.mp4", "D:\\tap.vi.mp4", false), "Video đã lồng, có chữ");
  assert.equal(showAsTextOnly("D:\\tap.vi.chu.mp4", "D:\\tap.vi.chu.mp4", false), true);
  assert.equal(showAsTextOnly("D:\\tap.vi.chu.mp4", "", false), true);
  assert.equal(watchHeading("D:\\tap.vi.mp4", "D:\\tap.vi.mp4", false), "Video đã lồng");
  assert.equal(showAsTextOnly("D:\\tap.vi.m4a", "", true), false);
});

test("Tải video saves the video on screen and never an audio file", () => {
  const cases: [string, string, string][] = [
    // [kết quả, bản đã cất đang xem, file sẽ lưu]
    ["D:\\a\\tap.vi.mp4", "", "D:\\a\\tap.vi.mp4"],
    ["D:\\a\\tap.vi.chu.mp4", "", "D:\\a\\tap.vi.chu.mp4"],
    ["D:\\a\\TAP.VI.MP4", "", "D:\\a\\TAP.VI.MP4"],
    ["D:\\a\\tap.vi.mp4", "D:\\a\\cu\\1\\tap.vi.mp4", "D:\\a\\cu\\1\\tap.vi.mp4"],
    ["D:\\a\\tap.vi.mp4", "D:\\a\\cu\\1\\tap.vi.m4a", ""],
    ["D:\\a\\tap.vi.m4a", "", ""],
    ["", "", ""],
  ];
  for (const [result, kept, want] of cases) {
    assert.equal(videoToSave(result, kept), want, `${result} | ${kept}`);
  }
});

test("changing only text mode is something to redo", () => {
  const same = {
    retranscribe: false,
    lang: "vi",
    currentLang: "vi",
    voice: "a",
    savedVoice: "a",
    names: [],
    base: [],
  };
  assert.equal(againPlan({ ...same, textOnly: false, wasTextOnly: false }), "Chưa đổi ngôn ngữ, giọng hay tên.");
  assert.notEqual(againPlan({ ...same, textOnly: true, wasTextOnly: false }), "Chưa đổi ngôn ngữ, giọng hay tên.");
  assert.notEqual(againPlan({ ...same, textOnly: false, wasTextOnly: true }), "Chưa đổi ngôn ngữ, giọng hay tên.");
  assert.equal(againPlan({ ...same, textOnly: true, wasTextOnly: true }), "Chưa đổi ngôn ngữ, giọng hay tên.");
});

test("an unfinished job says which stage it stopped at", () => {
  const none: DraftBrief = { heard: 0, translated: 0, spoken: 0, textOnly: false, excerpt: [], excerptTranslated: false };
  const cases: [Partial<DraftBrief>, string][] = [
    [{}, "Chưa chép lời."],
    [{ heard: 9 }, "Đã chép 9 câu, chưa dịch."],
    [{ heard: 9, translated: 4 }, "Đã chép 9 câu, dịch được 4."],
    [{ heard: 9, translated: 9 }, "Đã dịch 9 câu, chưa đọc giọng."],
    [{ heard: 9, translated: 9, spoken: 4 }, "Đã dịch 9 câu, mới đọc 4/9 câu."],
    [{ heard: 9, translated: 9, spoken: 9 }, "Đã dịch và đọc 9 câu, chưa ghép video."],
    // Việc chỉ ghi chữ không đọc giọng: file giọng có hay không cũng không đổi câu nói.
    [{ heard: 9, translated: 9, spoken: 9, textOnly: true }, "Đã dịch 9 câu, chưa ghi chữ vào video."],
    [{ heard: 9, translated: 4, textOnly: true }, "Đã chép 9 câu, dịch được 4."],
  ];
  for (const [change, want] of cases) {
    assert.equal(draftProgress({ ...none, ...change }), want);
  }
});
