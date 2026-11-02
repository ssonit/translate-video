import assert from "node:assert/strict";
import test from "node:test";
import { againPlan, draftProgress, freshSteps, mark, markText, maskHintShown, noticeFades, previewPath, showAsTextOnly, stepStateOf, videoToSave, watchHeading, overallEstimate, workflowPhases } from "./format.ts";
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

test("no time is promised while the running step has not measured any progress", () => {
  const stage = (progress: number | null) => freshSteps().map((step) => ({
    ...step,
    state: step.id === "extract" ? "done" : step.id === "transcribe" ? "running" : "pending",
    progress: step.id === "transcribe" ? progress : null,
  }));
  // Three finished seconds say nothing about the long step after them. Extrapolating from them
  // made the estimate grow with every second the step ran.
  assert.equal(overallEstimate(stage(null), 0, 6000).remaining, null);
  assert.equal(overallEstimate(stage(null), 0, 12000).remaining, null);
  assert.equal(overallEstimate(stage(0.01), 0, 12000).remaining, null);
  assert.ok(overallEstimate(stage(0.2), 0, 12000).remaining! > 0);
});

test("a step stopped by the user reads as stopped, not as an error", () => {
  assert.equal(stepStateOf("error", "Đã dừng."), "stopped");
  assert.equal(stepStateOf("error", "Không ghép được video."), "error");
  assert.equal(stepStateOf("running", "Đã dừng."), "running");
  assert.equal(markText("stopped"), "Đã dừng");
  assert.notEqual(mark("stopped"), mark("error"));
  const stoppedAtMerge = freshSteps().map((step) => ({ ...step, state: step.id === "merge" ? "stopped" : "done" }));
  assert.equal(workflowPhases(stoppedAtMerge)[3].state, "stopped");
  assert.equal(workflowPhases(stoppedAtMerge)[2].state, "done");
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

test("a finished-job notice fades by itself, a failure waits to be read", () => {
  assert.equal(noticeFades("Đã xử lý xong a.mp4.", false, ""), true);
  assert.equal(noticeFades("Đã dừng xử lý.", false, "Đã dừng."), true);
  assert.equal(noticeFades("Xử lý bị gián đoạn.", false, "Nguồn hỏng"), false);
  assert.equal(noticeFades("Đã bắt đầu a.mp4.", true, ""), false);
  assert.equal(noticeFades("", false, ""), false);
});

test("the hint about the source's own hard-coded text shows only when captions go on the picture and the cover is off", () => {
  const base = { audioOnly: false, textOnly: false, burnCaptions: false, maskOn: false };
  assert.equal(maskHintShown(base), false); // video lồng tiếng, phụ đề nằm trong file riêng
  assert.equal(maskHintShown({ ...base, burnCaptions: true }), true); // video lồng tiếng kèm phụ đề trên hình
  assert.equal(maskHintShown({ ...base, textOnly: true }), true); // Video phụ đề luôn ghi chữ lên hình
  assert.equal(maskHintShown({ ...base, audioOnly: true, burnCaptions: true }), false); // audio không có hình
  assert.equal(maskHintShown({ ...base, burnCaptions: true, maskOn: true }), false); // đã bật che rồi
  assert.equal(maskHintShown({ ...base, textOnly: true, maskOn: true }), false);
});

test("the layout preview video is held only while its page is open, and the job's source wins over a picked sample", () => {
  assert.equal(previewPath("D:/job/a.mp4", "D:/pick/b.mp4", true), "D:/job/a.mp4");
  assert.equal(previewPath("", "D:/pick/b.mp4", true), "D:/pick/b.mp4"); // việc bằng link: dùng video mẫu người dùng chọn
  assert.equal(previewPath("", "", true), "");
  assert.equal(previewPath("D:/job/a.mp4", "D:/pick/b.mp4", false), ""); // trang ẩn: thả cả hai
  assert.equal(previewPath("", "D:/pick/b.mp4", false), "");
});
