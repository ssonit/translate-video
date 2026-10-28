// Local checks only: no downloads, API calls, model inference or user settings.
import { readdirSync, mkdirSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";

const project = resolve(import.meta.dirname, "..");
process.chdir(project);
const reportDir = join(project, ".bench-asr", `verify-${Date.now()}`);
mkdirSync(reportDir, { recursive: true });
function testFiles(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap(entry => {
    const path = join(dir, entry.name);
    return entry.isDirectory() ? testFiles(path) : entry.name.endsWith(".test.ts") ? [path] : [];
  }).sort();
}
// Invoke npm's JS entry point directly so Windows cmd.exe does not parse paths.
const npm = process.env.npm_execpath;
if (!npm) throw new Error("Run with npm run verify:local");
const checks = [
  ["frontend", process.execPath, ["--experimental-strip-types", "--test", ...testFiles("src")]],
  ["queue", process.execPath, ["scripts/test-queue-actions.mjs"]],
  ["playback", process.execPath, ["scripts/test-playback-actions.mjs"]],
  ["windows-package", process.execPath, ["--test", "scripts/test-windows-release.mjs"]],
  ["build", process.execPath, [npm, "run", "build"]],
  ["rust", "cargo", ["test", "--manifest-path", "src-tauri/Cargo.toml", "--offline", "--lib"]],
];
const results = [];
for (const [name, command, args] of checks) {
  process.stdout.write(`Checking ${name}...\n`);
  const start = Date.now();
  const result = spawnSync(command, args, { encoding: "utf8", maxBuffer: 32 * 1024 * 1024 });
  const log = join(reportDir, `${name}.log`);
  writeFileSync(log, (result.stdout ?? "") + (result.stderr ?? "") + (result.error?.message ?? ""));
  results.push({ name, passed: result.status === 0, exitCode: result.status, elapsedMs: Date.now() - start, log });
  process.stdout.write(`${name}: ${result.status === 0 ? "PASS" : "FAIL"} (${log})\n`);
}
const report = join(reportDir, "report.json");
writeFileSync(report, JSON.stringify({ checkedAt: new Date().toISOString(), results,
  note: "Local regression only. Does not certify live providers, link downloads, speech quality, installers or a clean Windows machine." }, null, 2));
process.stdout.write(`Report: ${report}\n`);
if (results.some(result => !result.passed)) process.exitCode = 1;
