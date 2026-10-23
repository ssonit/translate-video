import assert from "node:assert/strict";
import test from "node:test";
import { DEEPSEEK_LOCAL_STEPS, GUIDES } from "./guides.ts";

test("model guide offers DeepSeek API with a key and DeepSeek local without one", () => {
  const model = GUIDES.find((guide) => guide.id === "model");
  assert.ok(model?.clouds);
  const api = model.clouds.find((cloud) => cloud.id === "deepseek");
  const local = model.clouds.find((cloud) => cloud.id === "deepseek-local");
  assert.ok(api?.keyUrl?.includes("platform.deepseek.com"));
  assert.ok(local);
  assert.equal(local.keyUrl, undefined);
  assert.notEqual(api.name, local.name);
  assert.match(DEEPSEEK_LOCAL_STEPS.join("\n"), /127\.0\.0\.1:8000/);
});

test("whisper guide keeps one slot and explains the GPU model", () => {
  const whisper = GUIDES.find((guide) => guide.id === "whisper");
  assert.ok(whisper);
  assert.equal(GUIDES.filter((guide) => guide.id === "whisper").length, 1);
  const text = whisper.steps.join("\n");
  assert.match(text, /py -3 -m pip install "faster-whisper>=1\.1\.0"/);
  assert.match(text, /whisper\\large-v3-turbo/);
  assert.match(text, /App không tự tải model GPU/);
  assert.ok(whisper.links.some((link) => link.url.includes("whisper-bin-x64.zip")));
  assert.ok(
    whisper.links.some((link) =>
      link.url.includes("huggingface.co/Systran/faster-whisper-large-v3-turbo"),
    ),
  );
});
