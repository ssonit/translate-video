import test from "node:test";
import assert from "node:assert/strict";
import { showListenStop } from "./listenStop.ts";

test("a listen pass can be stopped while it is reading", () => {
  assert.equal(showListenStop(true), true);
  assert.equal(showListenStop(false), false);
});
