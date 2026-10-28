// Exercise the shipped PowerShell preflight with isolated, disposable deliveries.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, readdirSync, rmdirSync, unlinkSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";

const project = resolve(import.meta.dirname, "..");
const script = join(project, "scripts/check-windows-release.ps1");
const config = JSON.parse(readFileSync(join(project, "src-tauri/tauri.conf.json"), "utf8"));
const required = ["tauri-app.exe", "HUONG-DAN.md", "check-windows-release.ps1", ...Object.values(config.bundle.resources)];

// Explicit unlink/rmdir also works on Unicode paths with the bundled Node runtime.
function removeFixture(dir) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) removeFixture(path);
    else unlinkSync(path);
  }
  rmdirSync(dir);
}

function fixture(fn) {
  const dir = mkdtempSync(join(tmpdir(), "release-kiểm tra "));
  try {
    const entries = required.map(path => {
      const content = Buffer.from(`fixture: ${path}`);
      mkdirSync(dirname(join(dir, path)), { recursive: true });
      writeFileSync(join(dir, path), content);
      return { path, bytes: content.length, sha256: createHash("sha256").update(content).digest("hex") };
    });
    const manifest = values => writeFileSync(join(dir, "manifest.json"), JSON.stringify(values));
    manifest(entries);
    const run = (...extra) => spawnSync("powershell.exe", ["-NoProfile", "-NonInteractive", "-File", script, "-Delivery", dir, ...extra], { encoding: "utf8" });
    fn({ dir, entries, manifest, run });
  } finally { removeFixture(dir); }
}

test("complete delivery verifies on a Unicode path and writes a new report", () => fixture(({ dir, run }) => {
  const report = join(dir, "report.json");
  const result = run("-ReportPath", report);
  assert.equal(result.status, 0, result.stderr);
  const saved = readFileSync(report, "utf8");
  const body = JSON.parse(saved);
  assert.equal(body.manifestPassed, true);
  assert.deepEqual(body.unlistedRequiredFiles, []);
  assert.equal(body.files.length, required.length);
  const again = run("-ReportPath", report);
  assert.notEqual(again.status, 0);
  assert.equal(readFileSync(report, "utf8"), saved);
}));

test("changed or missing resource fails validation", () => fixture(({ dir, run }) => {
  writeFileSync(join(dir, "tauri-app.exe"), "modified");
  unlinkSync(join(dir, "douyin/dtk/signing/native/sm3.py"));
  assert.equal(existsSync(join(dir, "douyin/dtk/signing/native/sm3.py")), false);
  const result = run();
  assert.equal(result.status, 1, result.stderr);
  const body = JSON.parse(result.stdout);
  assert.equal(body.manifestPassed, false);
  assert.ok(body.missingRequiredFiles.includes("douyin/dtk/signing/native/sm3.py"), result.stdout);
}));

test("required resource cannot be omitted from the manifest", () => fixture(({ entries, manifest, run }) => {
  manifest(entries.filter(entry => entry.path !== "douyin/dtk/signing/native/abogus.py"));
  const result = run();
  assert.equal(result.status, 1, result.stderr);
  const body = JSON.parse(result.stdout);
  assert.equal(body.manifestPassed, false);
  assert.deepEqual(body.unlistedRequiredFiles, ["douyin/dtk/signing/native/abogus.py"]);
}));

for (const [name, mutate] of [
  ["empty manifest", () => []],
  ["parent traversal", entries => [...entries, { ...entries[0], path: "../outside.exe" }]],
  ["absolute path", entries => [...entries, { ...entries[0], path: "C:/outside.exe" }]],
  ["duplicate path", entries => [...entries, { ...entries[0], path: "TAURI-APP.EXE" }]],
  ["missing hash", entries => entries.map((entry, i) => i ? entry : { path: entry.path, bytes: entry.bytes })],
  ["invalid byte count", entries => entries.map((entry, i) => i ? entry : { ...entry, bytes: -1 })],
]) {
  test(`${name} is rejected`, () => fixture(({ entries, manifest, run }) => {
    manifest(mutate(entries));
    const result = run();
    assert.notEqual(result.status, 0);
    assert.ok(result.stderr.length > 0);
  }));
}
