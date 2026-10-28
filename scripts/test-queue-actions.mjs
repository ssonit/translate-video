// Runs the real jobActions through Vite's TS loader; only the Tauri IPC boundary
// is replaced. No media, models, credentials or app storage are touched.
import assert from "node:assert/strict";
import test from "node:test";
import { createServer } from "vite";

const server = await createServer({ server: { middlewareMode: true }, appType: "custom" });
const { jobActions } = await server.ssrLoadModule("/src/work/job.ts");
const { DEFAULT_RENDER } = await server.ssrLoadModule("/src/work/renderSettings.ts");
const originals = ["window", "localStorage"].map(key => [key, Object.getOwnPropertyDescriptor(globalThis, key)]);
let ipc = async () => null;
Object.defineProperty(globalThis, "window", { configurable: true, value: { __TAURI_INTERNALS__: { invoke: (...args) => ipc(...args) } } });
const storage = new Map();
Object.defineProperty(globalThis, "localStorage", { configurable: true, value: { getItem: key => storage.get(key) ?? null, setItem: (key, value) => storage.set(key, value) } });

const item = source => ({ id: source, label: source, mode: "file", source, folder: "D:\\test", target: "vi", voice: "voice", audioOnly: false, textOnly: false, skipTranslate: true, burnCaptions: false, render: DEFAULT_RENDER, translation: { provider: "local", model: "qwen3.5:9b-q4_K_M", useTerms: false } });
function session(queue, policy) {
  const values = {
    queue, queueRef: { current: queue }, queuePolicy: policy, activeJob: { current: null },
    queueHistory: [], queueHistoryRef: { current: [] }, skipActive: { current: false },
    runPauseRef: { current: { pausedMs: 0, sinceMs: null } }, gate: { current: false },
    holdQueue: { current: false }, ownsRun: { current: false }, retryRef: { current: null },
    previewSlot: { current: { open: false, listen: false, epoch: 0 } },
    running: false, folder: "D:\\test", target: "vi", voice: "voice", model: "qwen3.5:9b-q4_K_M", provider: "local", useTerms: false,
    render: DEFAULT_RENDER, pending: "", mode: "file", video: "", link: "",
  };
  return new Proxy(values, { get(obj, key) {
    if (typeof key === "string" && key.startsWith("set")) {
      const field = key[3].toLowerCase() + key.slice(4);
      return value => { obj[field] = typeof value === "function" ? value(obj[field]) : value; };
    }
    return obj[key];
  } });
}

try {
  await test("continue-on-error records the failure and processes the following job", async () => {
    const calls = [];
    ipc = async (command, args) => {
      if (command === "finished_result") return "";
      if (command === "start_dub") { calls.push(args.video); if (args.video === "bad.mp4") throw "Nguồn hỏng"; return "D:\\test\\good.vi.mp4"; }
      return null;
    };
    const s = session([item("bad.mp4"), item("good.mp4")], "continue");
    await jobActions(s).runQueue();
    assert.deepEqual(calls, ["bad.mp4", "good.mp4"]);
    assert.deepEqual(s.queueHistoryRef.current.map(r => r.state), ["failed", "done"]);
    assert.equal(s.queueRef.current.length, 0); assert.equal(s.activeJob.current, null);
  });
  await test("stop retains the failure, then continuing only processes waiting jobs", async () => {
    const calls = [];
    ipc = async (command, args) => {
      if (command === "finished_result") return "";
      if (command === "start_dub") { calls.push(args.video); if (args.video === "bad.mp4") throw "Nguồn hỏng"; return "D:\\test\\good.vi.mp4"; }
      return null;
    };
    const s = session([item("bad.mp4"), item("good.mp4")], "stop");
    await jobActions(s).runQueue();
    assert.deepEqual(calls, ["bad.mp4"]);
    assert.equal(s.queueRef.current[0].source, "good.mp4"); assert.equal(s.activeJob.current.source, "bad.mp4");
    assert.equal(s.running, false);
    await jobActions(s).runQueue();
    assert.deepEqual(calls, ["bad.mp4", "good.mp4"]);
    assert.deepEqual(s.queueHistoryRef.current.map(r => r.state), ["failed", "done"]);
    assert.equal(s.queueRef.current.length, 0);
    assert.equal(s.activeJob.current, null);
    await jobActions(s).retryRecord(s.queueHistoryRef.current[0]);
    assert.deepEqual(calls, ["bad.mp4", "good.mp4", "bad.mp4"]);
    assert.deepEqual(s.queueHistoryRef.current.map(r => r.state), ["failed", "done", "failed"]);
  });
  await test("skip current advances without accepting names or treating the skip as a failure", async () => {
    const calls = []; let rejectCurrent;
    ipc = async (command, args) => {
      if (command === "finished_result") return "";
      if (command === "cancel_dub") { rejectCurrent("Đã dừng."); return null; }
      if (command === "start_dub") {
        calls.push(args.video);
        if (calls.length === 1) return new Promise((_, reject) => { rejectCurrent = reject; });
        return "D:\\test\\second.vi.mp4";
      }
      assert.notEqual(command, "confirm_names"); return null;
    };
    const s = session([item("first.mp4"), item("second.mp4")], "stop");
    const actions = jobActions(s); const pumping = actions.runQueue();
    for (let i = 0; !rejectCurrent && i < 30; i++) await new Promise(resolve => setTimeout(resolve, 5));
    assert.ok(rejectCurrent); await actions.skipCurrent(); await pumping;
    assert.deepEqual(calls, ["first.mp4", "second.mp4"]);
    assert.deepEqual(s.queueHistoryRef.current.map(r => r.state), ["skipped", "done"]);
    assert.equal(s.queueRef.current.length, 0);
  });
} finally {
  await server.close();
  for (const [key, descriptor] of originals) { if (descriptor) Object.defineProperty(globalThis, key, descriptor); else Reflect.deleteProperty(globalThis, key); }
}
