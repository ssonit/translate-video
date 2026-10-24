import assert from "node:assert/strict";
import test from "node:test";
import { appendJobs, parseSavedQueue, queueKey, queueLabel, restoreQueue, takeActive, type QueueJob } from "./queue.ts";

function job(source: string, mode: "file" | "link" = "file"): QueueJob {
  return {
    id: source,
    label: queueLabel(mode, source),
    mode,
    source,
    folder: "D:\\out",
    target: "vi",
    voice: "vi_female",
    audioOnly: false,
    skipTranslate: false,
    burnCaptions: false,
    textOnly: false,
  };
}

test("file label is the file name, link label keeps the id", () => {
  assert.equal(queueLabel("file", "D:\\phim\\tap-1.mp4"), "tap-1.mp4");
  assert.equal(queueLabel("link", "https://www.youtube.com/watch?v=abc"), "youtube.com · abc");
  assert.equal(queueLabel("link", "https://youtu.be/abc123"), "youtu.be · abc123");
});

test("the same file is not queued twice, and the running file is left out", () => {
  const first = appendJobs([], [job("D:\\a.mp4"), job("D:/a.mp4"), job("D:\\b.mp4")], queueKey("file", "D:\\b.mp4"));
  assert.deepEqual(
    first.added.map((item) => item.source),
    ["D:\\a.mp4"],
  );
  assert.equal(first.skipped, 2);
  const second = appendJobs(first.queue, [job("d:\\A.mp4"), job("https://youtu.be/abc", "link")]);
  assert.deepEqual(
    second.queue.map((item) => item.label),
    ["a.mp4", "youtu.be · abc"],
  );
});

// Cùng một video Douyin copy hai lần: mã đầu đoạn đổi mỗi lần, link thì giữ nguyên.
const DOUYIN_SHARE_A =
  "2.56 07/31 GiC:/ :7pm z@G.IV 【漂泊监狱】第二集 # 鸣潮  https://v.douyin.com/9F1g72OKQWY/ 复制此链接，打开Dou音搜索，直接观看视频！";
const DOUYIN_SHARE_B =
  "8.74 06/12 mNo:/ :4pm k@P.XQ 【漂泊监狱】第二集 # 鸣潮  https://v.douyin.com/9F1g72OKQWY/ 复制此链接，打开Dou音搜索，直接观看视频！";
const DOUYIN_OTHER =
  "3.10 08/01 aBc:/ :1pm x@Y.ZZ 【漂泊监狱】第三集  https://v.douyin.com/Ab3dEfGh1jK/ 复制此链接，打开Dou音搜索，直接观看视频！";

test("a share text is named by the link inside it, not by the text", () => {
  assert.equal(queueLabel("link", DOUYIN_SHARE_A), "v.douyin.com · 9F1g72OKQWY");
  assert.equal(
    queueLabel("link", "Xem video này https://vt.tiktok.com/ZS123abc/ trên TikTok"),
    "vt.tiktok.com · ZS123abc",
  );
});

test("the same video shared twice is queued once, and another video still gets in", () => {
  const result = appendJobs(
    [],
    [job(DOUYIN_SHARE_A, "link"), job(DOUYIN_SHARE_B, "link"), job(DOUYIN_OTHER, "link")],
  );
  assert.deepEqual(
    result.added.map((item) => item.source),
    [DOUYIN_SHARE_A, DOUYIN_OTHER],
  );
  assert.equal(result.skipped, 1);
});

test("the video that is running is not queued again as a share text", () => {
  const running = queueKey("link", "https://v.douyin.com/9F1g72OKQWY/");
  const result = appendJobs([], [job(DOUYIN_SHARE_B, "link")], running);
  assert.equal(result.added.length, 0);
  assert.equal(result.skipped, 1);
});

test("a saved queue without textOnly still loads as off", () => {
  const row = job("D:\\a.mp4");
  delete (row as { textOnly?: boolean }).textOnly;
  const saved = parseSavedQueue(JSON.stringify({ waiting: [row], current: null }));
  assert.equal(saved.waiting.length, 1);
  assert.equal(saved.waiting[0].textOnly, false);
  const on = parseSavedQueue(
    JSON.stringify({ waiting: [{ ...job("D:\\b.mp4"), textOnly: true }], current: null }),
  );
  assert.equal(on.waiting[0].textOnly, true);
});

test("a saved queue drops broken rows and keeps a readable title", () => {
  const saved = parseSavedQueue(
    JSON.stringify({
      waiting: [job("D:\\a.mp4"), { id: "x", mode: "link" }, job("https://youtu.be/abc", "link")],
      current: { ...job("D:\\dang-chay.mp4"), label: "Tập đang xem" },
    }),
  );
  assert.deepEqual(
    saved.waiting.map((item) => item.source),
    ["D:\\a.mp4", "https://youtu.be/abc"],
  );
  assert.equal(saved.current?.label, "Tập đang xem");
  assert.equal(parseSavedQueue("không phải json").waiting.length, 0);
  assert.equal(parseSavedQueue(null).current, null);
});

test("a video stopped by quitting returns to the front", () => {
  const saved = {
    waiting: [job("D:\\b.mp4"), job("D:\\a.mp4")],
    current: job("D:\\dang-chay.mp4"),
  };
  assert.deepEqual(
    restoreQueue(saved, false).map((item) => item.source),
    ["D:\\dang-chay.mp4", "D:\\b.mp4", "D:\\a.mp4"],
  );
  assert.deepEqual(
    restoreQueue(saved, true).map((item) => item.source),
    ["D:\\b.mp4", "D:\\a.mp4"],
  );
  assert.deepEqual(
    restoreQueue({ waiting: [job("D:\\a.mp4")], current: job("D:\\a.mp4") }, false).map((item) => item.source),
    ["D:\\a.mp4"],
  );
});

test("starting a video parks the previous one back on the queue", () => {
  const waiting = [job("D:\\a.mp4"), job("D:\\b.mp4")];
  const first = takeActive(waiting, null, waiting[0]);
  assert.deepEqual(
    first.waiting.map((item) => item.source),
    ["D:\\b.mp4"],
  );
  const second = takeActive(first.waiting, first.active, job("D:\\c.mp4"));
  assert.deepEqual(
    second.waiting.map((item) => item.source),
    ["D:\\a.mp4", "D:\\b.mp4"],
  );
  const retry = takeActive(second.waiting, second.active, job("D:\\c.mp4"));
  assert.deepEqual(
    retry.waiting.map((item) => item.source),
    ["D:\\a.mp4", "D:\\b.mp4"],
  );
});
