import assert from "node:assert/strict";
import test from "node:test";
import { filterLibrary } from "./libraryFilter.ts";
import type { FinishedDub } from "./types.ts";

const items: FinishedDub[] = [
  { title: "Chuyện biển", path: "done.mp4", video: "D:/nguon/tap-01.mp4", language: "vi", done: true, audio: false, captioned: true, modifiedMs: 2 },
  { title: "Video đang dở", path: "draft", video: "", url: "https://youtu.be/example", language: "en", done: false, audio: false, captioned: false, modifiedMs: 1 },
];

test("library filters completed and resumable jobs without dropping source metadata", () => {
  assert.deepEqual(filterLibrary(items, "", "all"), items);
  assert.deepEqual(filterLibrary(items, "", "draft"), [items[1]]);
  assert.deepEqual(filterLibrary(items, "", "done"), [items[0]]);
  assert.deepEqual(filterLibrary(items, "chuyen bien", "done"), [items[0]]);
  assert.deepEqual(filterLibrary(items, "tap-01", "all"), [items[0]]);
  assert.deepEqual(filterLibrary(items, "youtu.be", "draft"), [items[1]]);
  assert.deepEqual(filterLibrary(items, "youtu.be", "done"), []);
  assert.equal(items.length, 2);
});
