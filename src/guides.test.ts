import assert from "node:assert/strict";
import test from "node:test";
import { GUIDES } from "./guides.ts";

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
