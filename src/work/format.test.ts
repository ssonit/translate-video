import assert from "node:assert/strict";
import test from "node:test";
import { againPlan, freshSteps, showAsTextOnly, watchHeading } from "./format.ts";

test("text only skips the voice steps", () => {
  const steps = freshSteps(false, true);
  assert.equal(steps.find((step) => step.id === "speak")?.state, "skipped");
  assert.equal(steps.find((step) => step.id === "fit")?.state, "skipped");
  assert.equal(steps.find((step) => step.id === "merge")?.label, "Ghi chữ");
  const dubbed = freshSteps(false);
  assert.equal(dubbed.find((step) => step.id === "speak")?.state, "pending");
  assert.equal(dubbed.find((step) => step.id === "merge")?.label, "Ghép video");
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
