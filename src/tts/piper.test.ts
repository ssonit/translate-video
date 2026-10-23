import test from "node:test";
import assert from "node:assert/strict";
import { endMark, mergePhonemes } from "./phonemes.ts";

test("a passage keeps the period inside one phoneme string", () => {
  assert.equal(mergePhonemes("Một. Hai.", ["mot", "hai"]), "mot. hai");
});

test("a comma between clauses stays a comma", () => {
  assert.equal(mergePhonemes("Một, hai", ["mot", "hai"]), "mot, hai");
});

test("a single clause gains no extra mark", () => {
  assert.equal(mergePhonemes("Một.", ["mot"]), "mot");
});

test("a passage ending gains a pause so the last word can finish", () => {
  assert.equal(endMark("mot. hai"), "mot. hai.");
  assert.equal(endMark("mot."), "mot.");
  assert.equal(endMark(""), "");
});
