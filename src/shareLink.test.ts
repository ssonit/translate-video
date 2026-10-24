import assert from "node:assert/strict";
import test from "node:test";
import { shareLink } from "./shareLink.ts";

// Đoạn Douyin copy ra khi bấm chia sẻ: mã lạ, tiêu đề, hashtag, link, rồi câu hướng dẫn.
const DOUYIN_SHARE =
  "2.56 07/31 GiC:/ :7pm z@G.IV 【漂泊监狱】第二集 爱弥斯，什么叫我就是奖品？ 第二集终于来了，隔得太久我都不好意思了，后面会尽量快点更新的。感谢我鼠神的建议和指导\uFEFF@肥硕胖鑫鼠 \uFEFF ，砰砰砰。 # 鸣潮 # 抖音精选App征稿中 # 爱弥斯 # 鸣潮爱弥斯 # 漂泊者  https://v.douyin.com/9F1g72OKQWY/ 复制此链接，打开Dou音搜索，直接观看视频！";

test("a Douyin or TikTok share text gives back just its link", () => {
  const cases: [string, string][] = [
    [DOUYIN_SHARE, "https://v.douyin.com/9F1g72OKQWY/"],
    ["Xem video này https://vt.tiktok.com/ZS123abc/ trên TikTok", "https://vt.tiktok.com/ZS123abc/"],
    [
      "Hay quá\nhttps://www.tiktok.com/@ten.nguoi/video/7234567890123456789?is_from_webapp=1&sender_device=pc\n#fyp",
      "https://www.tiktok.com/@ten.nguoi/video/7234567890123456789?is_from_webapp=1&sender_device=pc",
    ],
    [" https://v.douyin.com/9F1g72OKQWY/\n", "https://v.douyin.com/9F1g72OKQWY/"],
  ];
  for (const [text, want] of cases) assert.equal(shareLink(text), want, text);
});

test("anything else gives nothing, so Rust judges the text as it did before", () => {
  for (const text of [
    "",
    "xem cái này đi",
    "https://",
    "https://youtu.be/abc123",
    "Hay lắm https://youtu.be/abc123",
    "Xem https://example.com rồi https://v.douyin.com/aaa/",
    "https://evildouyin.com/video/6961737553342991651",
    "https://v.douyin.com.evil.com/abc/",
    "https://douyin.com@evil.com/video/6961737553342991651",
  ]) {
    assert.equal(shareLink(text), null, text);
  }
});

test("punctuation stuck to the end of the link is not part of it", () => {
  assert.equal(shareLink("xem (https://vt.tiktok.com/ZS123abc/), hay lắm"), "https://vt.tiktok.com/ZS123abc/");
  assert.equal(shareLink("Link nè: https://v.douyin.com/9F1g72OKQWY/。"), "https://v.douyin.com/9F1g72OKQWY/");
});
