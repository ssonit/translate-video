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
    watchUpdateRef: { current: null }, viewRef: { current: "new" },
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
  await test("cancel preview preflight prevents Whisper from starting", async () => {
    let finish; const calls = [];
    ipc = async command => {
      calls.push(command);
      if (command === "finished_result") return new Promise(resolve => { finish = resolve; });
      return null;
    };
    const s = session([], "stop"); s.composeVideo = "video.mp4";
    const actions = jobActions(s); const pending = actions.runLive();
    for (let i = 0; !finish && i < 30; i++) await new Promise(resolve => setTimeout(resolve, 5));
    assert.ok(finish);
    assert.equal(s.running, true);
    await actions.stop(); finish(""); await pending;
    assert.equal(calls.includes("start_watch"), false);
    assert.equal(s.running, false); assert.equal(s.gate.current, false);
  });
  await test("cancel saved-video download prevents dubbing from starting", async () => {
    let finish; const calls = [];
    ipc = async command => {
      calls.push(command);
      if (command === "download_saved_video") return new Promise(resolve => { finish = resolve; });
      return null;
    };
    const s = session([], "stop"); s.result = "audio.vi.m4a";
    const actions = jobActions(s); const pending = actions.fetchVideo();
    for (let i = 0; !finish && i < 30; i++) await new Promise(resolve => setTimeout(resolve, 5));
    assert.ok(finish);
    await actions.stop(); finish("D:\\test\\video.mp4"); await pending;
    assert.equal(calls.includes("start_dub"), false);
    assert.equal(s.running, false); assert.equal(s.gate.current, false);
  });

  for (const phase of ["save_voice", "download_video", "finished_result"]) {
    await test(`cancel during ${phase} prevents the next pipeline from starting`, async () => {
      let finish; const calls = [];
      ipc = async command => {
        calls.push(command);
        if (command === phase) return new Promise(resolve => { finish = resolve; });
        if (command === "finished_result") return "";
        return null;
      };
      const queued = item("video.mp4");
      if (phase === "download_video") { queued.mode = "link"; queued.source = "https://youtu.be/test"; }
      const s = session([queued], "stop");
      const actions = jobActions(s); const pending = actions.runQueue();
      for (let i = 0; !finish && i < 30; i++) await new Promise(resolve => setTimeout(resolve, 5));
      assert.ok(finish);
      await actions.stop();
      assert.equal(s.running, true);
      finish(phase === "download_video" ? "D:\\test\\source.mp4" : "");
      await pending;
      assert.equal(calls.includes("start_dub"), false);
      assert.equal(s.running, false);
      assert.equal(s.gate.current, false);
      assert.equal(s.error, "Đã dừng.");
    });
  }

  await test("a running export opens progress immediately and switches to its result after completion", async () => {
    let finish;
    ipc = async command => {
      if (command === "finished_result") return "";
      if (command === "start_dub") return new Promise(resolve => { finish = resolve; });
      return null;
    };
    const s = session([item("video.mp4")], "stop");
    const pending = jobActions(s).runQueue();
    for (let i = 0; !finish && i < 30; i++) await new Promise(resolve => setTimeout(resolve, 5));
    assert.ok(finish);
    assert.equal(s.running, true);
    assert.equal(s.view, "progress");
    assert.match(s.runNotice, /Đã bắt đầu.*video.mp4/);
    finish("D:\\test\\video.vi.mp4");
    await pending;
    assert.equal(s.running, false);
    assert.equal(s.view, "watch");
    assert.equal(s.result, "D:\\test\\video.vi.mp4");
    assert.match(s.runNotice, /Hàng đợi đã hoàn tất/);
  });

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

  // Link không dùng được thì dừng ngay ở ô nhập: không chạy, không xếp hàng, không ghi lịch sử.
  const linkSource = (link, running = false) => {
    const s = session([], "stop"); s.mode = "link"; s.composeLink = link; s.running = running;
    return s;
  };
  await test("an invalid link is refused at the input before anything starts, queues or is recorded", async () => {
    const calls = [];
    ipc = async command => {
      calls.push(command);
      if (command === "check_link") throw "Chỉ nhận link YouTube, Douyin hoặc TikTok.";
      return null;
    };
    const s = linkSource("https://example.com/video");
    await jobActions(s).run();
    assert.deepEqual(calls, ["check_link"]);
    assert.equal(s.linkError, "Chỉ nhận link YouTube, Douyin hoặc TikTok.");
    assert.equal(s.composeLink, "https://example.com/video");
    assert.equal(s.running, false); assert.equal(s.gate.current, false);
    assert.equal(s.queueRef.current.length, 0); assert.equal(s.activeJob.current, null);
    assert.equal(s.queueHistoryRef.current.length, 0);
    assert.ok(!s.runNotice); assert.ok(!s.error);
  });
  await test("an invalid link is not added to the waiting list while another video runs", async () => {
    ipc = async command => { if (command === "check_link") throw "Link không đúng."; return null; };
    const s = linkSource("https://example.com/video", true);
    await jobActions(s).run();
    assert.equal(s.queueRef.current.length, 0);
    assert.equal(s.linkError, "Link không đúng.");
    assert.equal(s.composeLink, "https://example.com/video");
  });
  await test("a valid link clears the earlier link error and goes on to the download", async () => {
    const calls = [];
    ipc = async (command, args) => {
      calls.push(command === "check_link" ? `${command} ${args.url}` : command);
      return command === "download_video" ? "" : null;
    };
    const s = linkSource("https://youtu.be/abcdefghijk"); s.linkError = "Link không đúng.";
    await jobActions(s).run();
    assert.deepEqual(calls.filter(c => c !== "save_voice"), ["check_link https://youtu.be/abcdefghijk", "download_video"]);
    assert.equal(s.linkError, "");
  });

  await test("a link that finished downloading is still the running video and is not queued again", async () => {
    let finish;
    ipc = async command => {
      if (command === "download_video") return "D:\\test\\tai-ve\\clip [abcdefghijk].mp4";
      if (command === "start_dub") return new Promise(resolve => { finish = resolve; });
      return command === "finished_result" ? "" : null;
    };
    const s = linkSource("https://youtu.be/abcdefghijk");
    const actions = jobActions(s); const pending = actions.run();
    for (let i = 0; !finish && i < 30; i++) await new Promise(resolve => setTimeout(resolve, 5));
    assert.ok(finish); assert.equal(s.downloading, false);
    s.composeLink = "https://youtu.be/abcdefghijk";
    await actions.run();
    assert.equal(s.queueRef.current.length, 0);
    assert.match(s.queueNote, /đang chạy/);
    finish("D:\\test\\clip.vi.mp4"); await pending;
  });
  await test("retrying after a failure past the download keeps one entry for that video", async () => {
    let dubs = 0;
    ipc = async command => {
      if (command === "download_video") return "D:\\test\\tai-ve\\clip [abcdefghijk].mp4";
      if (command === "start_dub") { dubs += 1; if (dubs === 1) throw "Lỗi ghép"; return "D:\\test\\clip.vi.mp4"; }
      return command === "finished_result" ? "" : null;
    };
    const s = linkSource("https://youtu.be/abcdefghijk");
    const actions = jobActions(s);
    await actions.run();
    assert.equal(s.error, "Lỗi ghép");
    assert.equal(s.queueRef.current.length, 0);
    await actions.retryLast();
    assert.equal(dubs, 2);
    assert.equal(s.queueRef.current.length, 0);
    assert.equal(s.activeJob.current, null);
    assert.deepEqual(s.queueHistoryRef.current.map(r => r.state), ["failed", "done"]);
  });
  await test("starting another video after a failure does not bring the failed one back", async () => {
    const calls = [];
    ipc = async (command, args) => {
      if (command === "finished_result") return "";
      if (command === "start_dub") { calls.push(args.video); if (args.video === "bad.mp4") throw "Nguồn hỏng"; return "D:\\test\\good.vi.mp4"; }
      return null;
    };
    const s = session([], "stop"); s.composeVideo = "bad.mp4";
    const actions = jobActions(s);
    await actions.run();
    assert.equal(s.error, "Nguồn hỏng");
    s.composeVideo = "good.mp4";
    await actions.run();
    assert.deepEqual(calls, ["bad.mp4", "good.mp4"]);
    assert.equal(s.queueRef.current.length, 0); assert.equal(s.activeJob.current, null);
    assert.deepEqual(s.queueHistoryRef.current.map(r => r.state), ["failed", "done"]);
  });
  await test("running the same video again after a failure keeps that entry and the name it was given", async () => {
    let dubs = 0;
    ipc = async command => {
      if (command === "finished_result") return "";
      if (command === "start_dub") { dubs += 1; if (dubs === 1) throw "Nguồn hỏng"; return new Promise(() => {}); }
      return null;
    };
    const s = session([], "stop"); s.composeVideo = "bad.mp4";
    const actions = jobActions(s);
    await actions.run();
    s.activeJob.current = { ...s.activeJob.current, label: "Tên đã đặt" };
    const failedId = s.activeJob.current.id;
    s.composeVideo = "bad.mp4";
    void actions.run();
    for (let i = 0; dubs < 2 && i < 30; i++) await new Promise(resolve => setTimeout(resolve, 5));
    assert.equal(dubs, 2);
    assert.equal(s.activeJob.current.label, "Tên đã đặt");
    assert.equal(s.activeJob.current.id, failedId);
    assert.equal(s.queueRef.current.length, 0);
  });
  await test("exporting another video after a failure does not bring the failed one back either", async () => {
    const calls = [];
    ipc = async (command, args) => {
      if (command === "finished_result") return "";
      if (command === "start_dub") { calls.push(args.video); if (args.video === "bad.mp4") throw "Nguồn hỏng"; return "D:\\test\\good.vi.mp4"; }
      return null;
    };
    const s = session([], "stop"); s.composeVideo = "bad.mp4";
    const actions = jobActions(s);
    await actions.run();
    assert.equal(s.error, "Nguồn hỏng");
    await actions.runFrom("good.mp4");
    assert.deepEqual(calls, ["bad.mp4", "good.mp4"]);
    assert.equal(s.queueRef.current.length, 0); assert.equal(s.activeJob.current, null);
  });
  await test("a link run is announced by its queue label, never by the raw address", async () => {
    let finish;
    ipc = async command => {
      if (command === "download_video") return new Promise(resolve => { finish = resolve; });
      return null;
    };
    const s = linkSource("https://www.youtube.com/watch?v=jNQXAC9IVRw");
    const pending = jobActions(s).run();
    for (let i = 0; !finish && i < 30; i++) await new Promise(resolve => setTimeout(resolve, 5));
    assert.ok(finish);
    assert.match(s.runNotice, /youtube\.com · jNQXAC9IVRw/);
    assert.doesNotMatch(s.runNotice, /watch\?v=/);
    finish(""); await pending;
  });
  await test("a failed run keeps the notice short and puts the reason in the error only", async () => {
    ipc = async command => {
      if (command === "start_dub") throw "Nguồn hỏng";
      return command === "finished_result" ? "" : null;
    };
    const s = session([item("bad.mp4")], "stop");
    await jobActions(s).runQueue();
    assert.equal(s.error, "Nguồn hỏng");
    assert.equal(s.runNotice, "Xử lý bị gián đoạn.");
  });
} finally {
  await server.close();
  for (const [key, descriptor] of originals) { if (descriptor) Object.defineProperty(globalThis, key, descriptor); else Reflect.deleteProperty(globalThis, key); }
}
