import assert from "node:assert/strict";
import test from "node:test";
import { mergeNameRows, parseNamePaste, pasteSummary } from "./namePaste.ts";

test("pairs split on equals, arrow, and tab", () => {
  const parsed = parseNamePaste("Lina = Lynae\nLy Na → Lynae\nLina Na\tLynae\nBaku -> Baku");
  assert.deepEqual(parsed.pairs, [
    { from: "Lina", to: "Lynae" },
    { from: "Ly Na", to: "Lynae" },
    { from: "Lina Na", to: "Lynae" },
  ]);
  assert.equal(parsed.skippedMeanings, 0);
});

test("a repeated heard form is kept once", () => {
  const parsed = parseNamePaste("lina = Lynae\nLina = Lynae");
  assert.equal(parsed.pairs.length, 1);
});

test("a lore table stays out of the name list", () => {
  const parsed = parseNamePaste(
    [
      "1. Lynae",
      "2. Mornye",
      "Thuật ngữ\tÝ nghĩa",
      "Lahai-Roi\tVùng đất mới nằm sâu dưới Roya Frostlands, có hệ sinh thái độc đáo.",
      "Startorch Academy\tHọc viện kiêm đô thị nghiên cứu quy mô lớn tại Lahai-Roi.",
      "1. Thuật ngữ về thế giới và địa danh",
      "Wuthering Waves",
      "+1",
    ].join("\n"),
  );
  assert.deepEqual(parsed.pairs, []);
  assert.equal(parsed.skippedMeanings, 2);
  assert.ok(parsed.skippedNames >= 2);
  assert.match(pasteSummary(parsed), /Chưa thêm dòng nào/);
});

test("new pairs append and an existing heard form stays", () => {
  const rows = mergeNameRows(
    [
      { from: "", to: "" },
      { from: "Lina", to: "Lina" },
    ],
    [
      { from: "Lina", to: "Lynae" },
      { from: "Ly Na", to: "Lynae" },
    ],
  );
  assert.deepEqual(rows, [
    { from: "Lina", to: "Lina" },
    { from: "Ly Na", to: "Lynae" },
  ]);
});
