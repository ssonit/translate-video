import assert from "node:assert/strict";
import { test } from "node:test";
import { recoverSpeech } from "./recover.ts";

test("a stalled sentence is restarted once, without losing the following sentence", async () => {
  let attempts = 0, resets = 0;
  const first = await recoverSpeech(() => {
    attempts += 1;
    return attempts === 1 ? new Promise<string>(() => {}) : Promise.resolve("first PCM");
  }, async () => { resets += 1; }, () => false, 10);
  const next = await recoverSpeech(async () => "next PCM", async () => { throw Error("unexpected reset"); }, () => false, 10);
  assert.equal(first, "first PCM"); assert.equal(next, "next PCM");
  assert.equal(attempts, 2); assert.equal(resets, 1);
});

test("actual model failures and cancelled work never restart", async () => {
  let resets = 0;
  const reset = async () => { resets += 1; };
  await assert.rejects(recoverSpeech(async () => { throw Error("invalid voice"); }, reset, () => false, 10), /invalid voice/);
  await assert.rejects(recoverSpeech(() => new Promise(() => {}), reset, () => true, 10), /không trả lời/);
  assert.equal(resets, 0);
});

test("a second stall fails and cancellation during reload prevents a second read", async () => {
  let resets = 0;
  await assert.rejects(recoverSpeech(() => new Promise(() => {}), async () => { resets += 1; }, () => false, 10), /không trả lời/);
  assert.equal(resets, 1);
  let cancelled = false, reads = 0;
  await assert.rejects(recoverSpeech(() => { reads += 1; return new Promise(() => {}); }, async () => { cancelled = true; }, () => cancelled, 10), /Đã dừng/);
  assert.equal(reads, 1);
});
