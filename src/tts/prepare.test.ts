import test from "node:test";
import assert from "node:assert/strict";
import { listenPieces } from "./prepare.ts";

test("the first sentence is ready before the rest of the part", () => {
  assert.deepEqual(listenPieces("Một hai. Ba bốn."), ["Một hai.", "Ba bốn."]);
});

test("a line break is its own piece", () => {
  assert.deepEqual(listenPieces("Câu đầu\n\nCâu sau. Hết."), ["Câu đầu", "Câu sau.", "Hết."]);
});

test("blank lines add no piece", () => {
  assert.deepEqual(listenPieces("\n  \nMột."), ["Một."]);
});

test("an empty part has no sentence to speak", () => {
  assert.deepEqual(listenPieces("  \n"), []);
});
