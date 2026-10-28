import assert from "node:assert/strict";
import test from "node:test";
import { cleanRender, DEFAULT_RENDER, readPresets, readRender } from "./renderSettings.ts";

test("corrupt style input falls back safely without persisting unknown fields", () => {
  const style = cleanRender({ aspect: "bad", font: "Arial,code", color: "#invalid", size: Infinity,
    focusX: -80, captionY: 0, secret: "discard" });
  assert.equal(style.aspect, "original");
  assert.equal(style.font, "Arial");
  assert.equal(style.color, DEFAULT_RENDER.color);
  assert.equal(style.size, DEFAULT_RENDER.size);
  assert.equal(style.focusX, 0);
  assert.equal(style.captionY, 20);
  assert.ok(!JSON.stringify(style).includes("secret"));
});

test("channel presets reload settings and ignore corrupt rows and stray credentials", () => {
  const original = Object.getOwnPropertyDescriptor(globalThis, "localStorage");
  let raw = JSON.stringify([{ id: "channel", name: "Kênh của tôi", target: "vi", voice: "voice-a",
    render: { ...DEFAULT_RENDER, aspect: "9:16", color: "#ffcc00" }, audioOnly: true, textOnly: true,
    burnCaptions: true, skipTranslate: false, useTerms: true, apiKey: "discard" }, { name: "broken" }]);
  Object.defineProperty(globalThis, "localStorage", { configurable: true, value: { getItem: () => raw } });
  try {
    const presets = readPresets();
    assert.equal(presets.length, 1);
    assert.equal(presets[0].render.aspect, "9:16");
    assert.equal(presets[0].voice, "voice-a");
    assert.equal(presets[0].useTerms, true);
    assert.equal(presets[0].audioOnly, true);
    assert.equal(presets[0].textOnly, false);
    assert.ok(!JSON.stringify(presets).includes("apiKey"));
    raw = "bad JSON";
    assert.deepEqual(readPresets(), []);
    assert.deepEqual(readRender(), DEFAULT_RENDER);
  } finally {
    if (original) Object.defineProperty(globalThis, "localStorage", original);
    else Reflect.deleteProperty(globalThis, "localStorage");
  }
});
