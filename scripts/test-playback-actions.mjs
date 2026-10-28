// Exercise real playback actions with delayed IPC; never open media or dialogs.
import assert from "node:assert/strict";
import test from "node:test";
import { createServer } from "vite";

const server = await createServer({ server: { middlewareMode: true }, appType: "custom" });
const { playbackActions } = await server.ssrLoadModule("/src/work/playback.ts");
const original = Object.getOwnPropertyDescriptor(globalThis, "window");
let ipc;
Object.defineProperty(globalThis, "window", { configurable: true, value: { __TAURI_INTERNALS__: { invoke: (...args) => ipc(...args) } } });
function session() {
  const s = {
    result: "D:/jobs/a/a.vi.mp4", listenFile: "", playingKept: "", openNote: "",
    running: false, joining: false, captioning: false, ownsRun: { current: false },
    versionSelection: { current: { result: "D:/jobs/a/a.vi.mp4", busy: false, serial: 0 } },
  };
  s.setPlayingKept = value => { s.playingKept = value; };
  s.setOpenNote = value => { s.openNote = value; };
  return s;
}
try {
  await test("the latest version click wins even when IPC resolves in reverse order", async () => {
    const replies = new Map();
    ipc = (command, args) => { assert.equal(command, "select_version"); return new Promise(resolve => replies.set(args.path, resolve)); };
    const s = session(), actions = playbackActions(s);
    const first = actions.selectVersion("old-a.mp4"), second = actions.selectVersion("old-b.mp4");
    replies.get("old-b.mp4")("resolved-b.mp4"); await second;
    replies.get("old-a.mp4")("resolved-a.mp4"); await first;
    assert.equal(s.playingKept, "resolved-b.mp4");
    const back = actions.selectVersion(""); replies.get(s.result)(s.result); await back;
    assert.equal(s.playingKept, "");
  });
  await test("a late selection cannot replace a new job or a player that became busy", async () => {
    for (const change of [s => { s.versionSelection.current.result = "D:/jobs/b/b.vi.mp4"; }, s => { s.versionSelection.current.busy = true; }, s => { s.ownsRun.current = true; }]) {
      let resolve; ipc = () => new Promise(done => { resolve = done; });
      const s = session(), selected = playbackActions(s).selectVersion("old.mp4");
      change(s); resolve("old.mp4"); await selected;
      assert.equal(s.playingKept, ""); assert.equal(s.openNote, "");
    }
  });
  await test("stale errors are ignored while current errors remain visible", async () => {
    let reject; ipc = () => new Promise((_, fail) => { reject = fail; });
    const s = session(); const stale = playbackActions(s).selectVersion("old.mp4");
    s.versionSelection.current.result = "another-job"; reject("missing old file"); await stale;
    assert.equal(s.openNote, "");
    s.versionSelection.current.result = s.result;
    const current = playbackActions(s).selectVersion("old.mp4"); reject("File không còn tồn tại."); await current;
    assert.equal(s.openNote, "File không còn tồn tại.");
  });
} finally {
  await server.close();
  if (original) Object.defineProperty(globalThis, "window", original); else delete globalThis.window;
}
