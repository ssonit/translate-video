import assert from "node:assert/strict";
import test from "node:test";
import { splitSuggestion, structureTextProblem } from "./structure.ts";
import { pickedEdits } from "./format.ts";

test("split suggestions preserve graphemes and require manual completion of short text", () => {
  for (const text of ["Tiếng Việt và tên Toronto", "汉字字幕没有空格", "👨‍👩‍👧‍👦🇻🇳👍🏽🙂"]) {
    const [left, right] = splitSuggestion(text, .5);
    assert.equal((left + right).replace(/\s/g, ""), text.replace(/\s/g, ""));
    assert.ok(!left.endsWith("\u200d") && !right.startsWith("\u200d"));
  }
  assert.deepEqual(splitSuggestion("A", .5), ["A", ""]);
  assert.notEqual(structureTextProblem("A", ""), "");
  assert.equal(structureTextProblem("First", "Phần đầu", "Second", "Phần sau"), "");
});

test("later text/timing requests carry the stable cue id after an index shifts", () => {
  const lines = [{ start: 0, end: 1, source: "a", translation: "A", id: "cue-000003", revision: "current" }];
  const edits = pickedEdits(lines, { 0: { text: "B", again: false } });
  assert.equal(edits[0].id, "cue-000003");
  assert.equal(edits[0].index, 0);
  assert.equal(edits[0].revision, "current");
});
