import assert from "node:assert/strict";
import test from "node:test";
import { createServer } from "vite";

const server = await createServer({ server: { middlewareMode: true }, appType: "custom" });
const { playVoicePreview } = await server.ssrLoadModule("/src/tts/voicePreview.ts");
const originalWindow = Object.getOwnPropertyDescriptor(globalThis, "window");
const originalAudio = Object.getOwnPropertyDescriptor(globalThis, "Audio");
let ipc;
let players;
Object.defineProperty(globalThis, "window", { configurable: true, value: { __TAURI_INTERNALS__: { invoke: (...args) => ipc(...args) } } });
Object.defineProperty(globalThis, "Audio", { configurable: true, value: class {
  paused = false;
  constructor() { players.push(this); }
  play() { return Promise.resolve(); }
  pause() { this.paused = true; }
  removeAttribute() {}
} });

try {
  await test("cancel before starting sends no synthesis request", async () => {
    ipc = () => { assert.fail("No request should start"); };
    players = [];
    const controller = new AbortController(); controller.abort();
    await playVoicePreview("vi", "test", controller.signal);
    assert.equal(players.length, 0);
  });
  await test("cancel synthesis holds the request until it settles and never plays its late result", async () => {
    const calls = []; let finish;
    ipc = async command => {
      calls.push(command);
      if (command === "preview_voice") return new Promise(resolve => { finish = resolve; });
    };
    players = [];
    const controller = new AbortController(); let settled = false;
    const pending = playVoicePreview("vi", "test", controller.signal).then(() => { settled = true; });
    controller.abort();
    await Promise.resolve();
    assert.equal(settled, false);
    assert.deepEqual(calls, ["preview_voice", "cancel_dub"]);
    finish(new ArrayBuffer(8));
    await pending;
    assert.equal(players.length, 0);
  });
  await test("stop during playback pauses immediately without canceling another Rust task", async () => {
    const calls = [];
    ipc = async command => { calls.push(command); return new ArrayBuffer(8); };
    players = [];
    const controller = new AbortController();
    const pending = playVoicePreview("vi", "test", controller.signal);
    for (let i = 0; !players.length && i < 20; i++) await new Promise(resolve => setTimeout(resolve, 1));
    assert.equal(players.length, 1);
    controller.abort();
    assert.equal(players[0].paused, true);
    await pending;
    assert.deepEqual(calls, ["preview_voice"]);
  });
} finally {
  if (originalWindow) Object.defineProperty(globalThis, "window", originalWindow); else delete globalThis.window;
  if (originalAudio) Object.defineProperty(globalThis, "Audio", originalAudio); else delete globalThis.Audio;
  await server.close();
}
