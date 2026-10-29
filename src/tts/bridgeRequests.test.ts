import assert from "node:assert/strict";
import { test } from "node:test";
import { bridgeRequests, type SpeechRequest } from "./bridgeRequests.ts";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(done => { resolve = done; });
  return { promise, resolve };
}
const pcm = { rate: 22050, samples: new Int16Array([1, 2]) };
const request = (session: string): SpeechRequest => ({ session, lang: "vi", voice: "voice", items: [{ index: 1, text: session }] });

test("cancel during model load never starts speaking or saving the abandoned request", async () => {
  const load = deferred<void>();
  const saves: string[] = [], finishes: string[] = [];
  let reads = 0;
  const read = async () => { reads += 1; return pcm; };
  const bridge = bridgeRequests({ load: () => load.promise, speak: read, speakPassage: read, stop() {} },
    async session => { saves.push(session); }, async session => { finishes.push(session); });
  const old = bridge.handle(request("old"));
  bridge.cancel();
  load.resolve();
  await old;
  assert.equal(reads, 0);
  assert.deepEqual(saves, []);
  assert.deepEqual(finishes, ["old"]);
});

test("late PCM, cleanup and scoped cancel from an old session cannot affect the next job", async () => {
  const oldPcm = deferred<typeof pcm>(), nextPcm = deferred<typeof pcm>();
  const oldStarted = deferred<void>(), nextStarted = deferred<void>();
  const saves: string[] = [], finishes: string[] = [];
  let stops = 0;
  const read = async (text: string) => {
    if (text === "old") { oldStarted.resolve(); return oldPcm.promise; }
    nextStarted.resolve(); return nextPcm.promise;
  };
  const bridge = bridgeRequests({ load: async () => {}, speak: read, speakPassage: read, stop() { stops += 1; } },
    async session => { saves.push(session); }, async session => { finishes.push(session); });
  const old = bridge.handle(request("old"));
  await oldStarted.promise;
  bridge.cancel();
  const next = bridge.handle(request("next"));
  await nextStarted.promise;
  oldPcm.resolve(pcm);
  await old;
  bridge.cancel("old");
  assert.equal(stops, 1);
  // Old finally must not unlock the new request while it is still reading.
  await bridge.handle(request("third"));
  assert.deepEqual(saves, []);
  nextPcm.resolve(pcm);
  await next;
  assert.deepEqual(saves, ["next"]);
  assert.deepEqual(finishes, ["old", "third", "next"]);
});

test("a failed WAV commit stops the batch and reports failure under its own session", async () => {
  const saved: number[] = [], finished: [string, string | null][] = [];
  const read = async () => pcm;
  const bridge = bridgeRequests({ load: async () => {}, speak: read, speakPassage: read, stop() {} },
    async (_session, index) => { saved.push(index); throw Error("disk full"); },
    async (session, error) => { finished.push([session, error]); });
  await bridge.handle({ ...request("job"), items: [{ index: 1, text: "một" }, { index: 2, text: "hai" }] });
  assert.deepEqual(saved, [1]);
  assert.deepEqual(finished, [["job", "disk full"]]);
});
