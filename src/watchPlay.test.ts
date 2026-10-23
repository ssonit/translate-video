import assert from "node:assert/strict";
import test from "node:test";
import {
  DUCK_VOLUME,
  advanceClock,
  audioMove,
  frame,
  listenErrorDetail,
  playerErrorDetail,
  previewStillOpen,
  voiceGate,
  wantsPlay,
} from "./watchPlay.ts";

const lines = [
  { start: 1, end: 4 },
  { start: 6, end: 9 },
];

test("text only preview keeps the original volume", () => {
  const shot = frame(0.5, 10, true, [{ start: 0, end: 2 }], false);
  assert.equal(shot.volume, 1);
  assert.equal(shot.line, 0);
});

test("seek past the ready point snaps back while the run is going", () => {
  const got = frame(20, 12, true, lines);
  assert.equal(got.time, 12);
});

test("a line ducks and a gap between lines does not", () => {
  const inside = frame(2, 30, true, lines);
  assert.equal(inside.line, 0);
  assert.equal(inside.volume, DUCK_VOLUME);
  assert.equal(inside.hold, false);
  const gap = frame(5, 30, true, lines);
  assert.equal(gap.line, null);
  assert.equal(gap.volume, 1);
});

test("holding the lead restores the original volume", () => {
  const got = frame(11.2, 12, true, [{ start: 10, end: 12 }]);
  assert.equal(got.hold, true);
  assert.equal(got.volume, 1);
});

test("after the run finishes, seeking ahead is allowed and the tail is not held", () => {
  const got = frame(40, 12, false, lines);
  assert.equal(got.time, 40);
  assert.equal(got.hold, false);
  assert.equal(got.volume, 1);
});

test("a user pause is not cleared when the hold ends", () => {
  assert.equal(wantsPlay(false, true, true), false);
  assert.equal(wantsPlay(true, false, true), false);
  assert.equal(wantsPlay(false, false, true), true);
  assert.equal(wantsPlay(false, false, false), false);
});

test("a short clip that already ended is not started again", () => {
  const move = audioMove(3.2, 1, false, { currentTime: 1.2, duration: 1.2, ended: true });
  assert.equal(move, "stop");
});

test("a new line starts even if the previous clip had ended", () => {
  const move = audioMove(6.2, 6, true, { currentTime: 1.2, duration: 1.2, ended: true });
  assert.equal(move, "start");
});

test("drift inside the clip seeks, drift past the clip stops", () => {
  assert.equal(audioMove(2.5, 1, false, { currentTime: 0.2, duration: 8, ended: false }), "seek");
  assert.equal(audioMove(2.1, 1, false, { currentTime: 1.0, duration: 8, ended: false }), "keep");
  assert.equal(audioMove(10, 1, false, { currentTime: 1, duration: 2, ended: false }), "stop");
});

test("a clip that still has words holds the picture at the end of the line", () => {
  const head = { currentTime: 1.9, duration: 4, ended: false };
  assert.equal(voiceGate(1.9, 2, head, false), "finish");
  assert.equal(voiceGate(0.4, 2, head, false), "play");
});

test("finishing keeps going until the last words are out", () => {
  assert.equal(voiceGate(1.9, 2, { currentTime: 2.4, duration: 4, ended: false }, true), "finish");
  assert.equal(voiceGate(1.9, 2, { currentTime: 3.95, duration: 4, ended: false }, true), "done");
  assert.equal(voiceGate(1.9, 2, { currentTime: 4, duration: 4, ended: true }, true), "done");
});

test("a clip that fits the line is not held", () => {
  assert.equal(voiceGate(1.9, 2, { currentTime: 1.85, duration: 1.9, ended: false }, false), "play");
  assert.equal(voiceGate(1.9, 2, { currentTime: 0, duration: Number.NaN, ended: false }, false), "play");
});

test("a file the player cannot open explains that the picture is not here", () => {
  assert.equal(playerErrorDetail(), "Chưa có hình để xem trước. Bản chỉ audio nghe được khi chạy xong.");
});

test("a listen clock moves only while it is playing, and stops at the lead", () => {
  assert.equal(advanceClock(10, 0.5, 30, true, true), 10.5);
  assert.equal(advanceClock(11.8, 0.5, 12, true, true), 12);
  assert.equal(advanceClock(10, 0.5, 30, true, false), 10);
  assert.equal(advanceClock(11.8, 2, 12, false, true), 13.8);
});

test("audio that will not play says so without asking for a picture", () => {
  assert.equal(listenErrorDetail(), "Không phát được tiếng này.");
});

test("a preview stays open while it runs, after it publishes, or while lines remain", () => {
  assert.equal(previewStillOpen({ running: true, published: false, lines: [] }), true);
  assert.equal(previewStillOpen({ running: false, published: true, lines: [] }), true);
  assert.equal(previewStillOpen({ running: false, published: false, lines: [{}] }), true);
  assert.equal(previewStillOpen({ running: false, published: false, lines: [] }), false);
});
