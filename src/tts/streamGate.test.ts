import assert from "node:assert/strict";
import test from "node:test";
import { acceptChunk, enqueueSpeak, loadJoin, loadStillCurrent, slotAfterCrash } from "./streamGate.ts";

test("a second load of the voice already in memory does nothing", () => {
  assert.equal(loadJoin("vi/nam", "", "vi/nam"), "ready");
});

test("a second load of the voice still loading joins that load", () => {
  assert.equal(loadJoin("", "vi/nam", "vi/nam"), "join");
});

test("a different voice starts a new load", () => {
  assert.equal(loadJoin("vi/nam", "", "vi/nu"), "start");
  assert.equal(loadJoin("", "", "vi/nam"), "start");
});

test("a chunk from an older click is dropped", () => {
  assert.equal(acceptChunk(1, 2), false);
  assert.equal(acceptChunk(2, 2), true);
});

test("a load that was stopped does not open a session", () => {
  assert.equal(loadStillCurrent(1, 2), false);
  assert.equal(loadStillCurrent(2, 2), true);
});

test("a second speak waits until the first session returns", async () => {
  let running = 0;
  let peak = 0;
  let tail = Promise.resolve();
  const start = () => {
    tail = enqueueSpeak(tail, async () => {
      running += 1;
      peak = Math.max(peak, running);
      await new Promise((resolve) => setTimeout(resolve, 20));
      running -= 1;
    });
  };
  start();
  start();
  await tail;
  assert.equal(peak, 1);
});

test("a crashed worker is not a loaded voice", () => {
  assert.deepEqual(slotAfterCrash({ voice: "vi/nam" }), { alive: false, voice: "" });
});
