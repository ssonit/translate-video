import assert from "node:assert/strict";
import { test } from "node:test";
import { playbackInterrupted, regionQueue, splitText } from "./demand.ts";

function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(done => { resolve = done; }); return { promise, resolve }; }
const flush = () => new Promise<void>(done => setImmediate(done));

test("seeking keeps one running region and replaces all queued regions with the newest", async () => {
  const first = deferred<number>(); const runs: string[] = [], values: number[] = [];
  const queue = regionQueue({ publish: async () => {}, run: async (_root, serial) => { runs.push(serial); return runs.length === 1 ? first.promise : 2; },
    accept: value => values.push(value), fail: error => { throw error; }, busy() {}, cancel: async () => {} }, "q");
  queue.request({ root: "video", start: 0, end: 30, listen: true }); await flush();
  queue.request({ root: "video", start: 50, end: 80, listen: true });
  queue.request({ root: "video", start: 100, end: 130, listen: false });
  first.resolve(1); await queue.idle();
  assert.deepEqual(runs, ["q:1", "q:3"]); assert.deepEqual(values, [1, 2]);
});

test("pause permits the current batch to commit and discards queued work", async () => {
  const done = deferred<number>(); let runs = 0; const values: number[] = [];
  const queue = regionQueue({ publish: async () => {}, run: async () => { runs += 1; return done.promise; }, accept: v => values.push(v),
    fail: error => { throw error; }, busy() {}, cancel: async () => {} }, "q");
  queue.request({ root: "video", start: 0, end: 30, listen: false }); await flush();
  queue.request({ root: "video", start: 30, end: 60, listen: false });
  await queue.pause(); done.resolve(1); await queue.idle();
  assert.equal(runs, 1); assert.deepEqual(values, [1]);
});

test("closing a source revokes it and never accepts late responses", async () => {
  const done = deferred<number>(); const values: number[] = []; let cancelled = 0;
  const queue = regionQueue({ publish: async () => {}, run: async () => done.promise, accept: v => values.push(v), fail() {}, busy() {},
    cancel: async () => { cancelled += 1; done.resolve(1); } }, "q");
  queue.request({ root: "old", start: 0, end: 30, listen: true }); await flush();
  await queue.close(); queue.request({ root: "new", start: 0, end: 30, listen: true });
  assert.equal(cancelled, 1); assert.deepEqual(values, []);
});

test("a new seek arriving before IPC starts prevents the old region from running", async () => {
  const published = deferred<void>(); const runs: string[] = []; let calls = 0;
  const queue = regionQueue({ publish: async () => { if (++calls === 1) await published.promise; }, run: async (_root, serial) => { runs.push(serial); return serial; },
    accept() {}, fail: error => { throw error; }, busy() {}, cancel: async () => {} }, "q");
  queue.request({ root: "video", start: 0, end: 30, listen: false }); await flush();
  queue.request({ root: "video", start: 80, end: 110, listen: false }); published.resolve(); await queue.idle();
  assert.deepEqual(runs, ["q:2"]);
});

test("text splitting never loses words", () => {
  const [a, b] = splitText("one two three four", 0.5);
  assert.equal(`${a} ${b}`, "one two three four"); assert.ok(a); assert.ok(b);
  assert.deepEqual(splitText("single", 0.5), ["single", ""]);
});

test("a play interrupted by pause is expected but playback permission and decode failures are not", () => {
  assert.equal(playbackInterrupted(new DOMException("interrupted by pause", "AbortError")), true);
  assert.equal(playbackInterrupted(new DOMException("permission denied", "NotAllowedError")), false);
  assert.equal(playbackInterrupted(new DOMException("decode failed", "NotSupportedError")), false);
  assert.equal(playbackInterrupted(new Error("media failed")), false);
  assert.equal(playbackInterrupted(null), false);
});

test("an obsolete region failure cannot stop a newer seek", async () => {
  const first = deferred<void>(); const failures: unknown[] = [], values: number[] = []; let runs = 0;
  const queue = regionQueue({ publish: async () => {}, run: async () => {
    if (++runs === 1) { await first.promise; throw Error("Vùng yêu cầu đã đổi."); } return 2;
  }, accept: value => values.push(value), fail: error => failures.push(error), busy() {}, cancel: async () => {} }, "q");
  queue.request({ root: "video", start: 0, end: 30, listen: false }); await flush();
  queue.request({ root: "video", start: 50, end: 80, listen: true });
  first.resolve(); await queue.idle();
  assert.deepEqual(failures, []); assert.deepEqual(values, [2]);
});

test("pause suppresses a late failure but current failures remain visible", async () => {
  const first = deferred<void>(); const failures: unknown[] = []; let runs = 0;
  const queue = regionQueue({ publish: async () => {}, run: async () => {
    if (++runs === 1) await first.promise; throw Error("failure");
  }, accept() {}, fail: error => failures.push(error), busy() {}, cancel: async () => {} }, "q");
  queue.request({ root: "video", start: 0, end: 30, listen: true }); await flush();
  await queue.pause(); first.resolve(); await queue.idle();
  assert.equal(failures.length, 0);
  queue.request({ root: "video", start: 50, end: 80, listen: true }); await queue.idle();
  assert.equal(failures.length, 1);
});
