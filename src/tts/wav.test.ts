import test from "node:test";
import assert from "node:assert/strict";
import { wavBytes } from "./wav.ts";

test("a pcm buffer becomes a mono wav the browser can play", () => {
  const samples = Int16Array.from([0, 1000, -1000]);
  const bytes = wavBytes(22050, samples);
  assert.equal(bytes.length, 44 + 6);
  assert.equal(String.fromCharCode(...bytes.slice(0, 4)), "RIFF");
  assert.equal(String.fromCharCode(...bytes.slice(8, 12)), "WAVE");
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  assert.equal(view.getUint32(24, true), 22050);
  assert.equal(view.getUint16(22, true), 1);
  assert.equal(view.getInt16(44, true), 0);
  assert.equal(view.getInt16(46, true), 1000);
  assert.equal(view.getInt16(48, true), -1000);
});
