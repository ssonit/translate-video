import type { LinePick, ScriptLine } from "./types.ts";

export function cueTimes(line: ScriptLine, pick?: LinePick) {
  return { start: pick?.start ?? line.start, end: pick?.end ?? line.end };
}

export function timingProblem(lines: ScriptLine[], picks: Record<number, LinePick>, index: number, start: number, end: number, duration: number) {
  if (![start, end, duration].every(Number.isFinite) || start < 0 || end - start < 0.079999 || end > duration + 0.001) {
    return "Chọn mốc trong video, dài ít nhất 0,08 giây.";
  }
  const before = lines.map(line => ({ start: line.start, end: line.end }));
  const next = lines.map((line, i) => cueTimes(line, picks[i]));
  next[index] = { start, end };
  if ((index > 0 && start < next[index - 1].start) || (index + 1 < next.length && start > next[index + 1].start)) {
    return "Không di chuyển qua câu bên cạnh.";
  }
  for (let j = 0; j < next.length; j++) {
    if (j === index) continue;
    const overlap = Math.max(0, Math.min(end, next[j].end) - Math.max(start, next[j].start));
    const old = Math.max(0, Math.min(before[index].end, before[j].end) - Math.max(before[index].start, before[j].start));
    if (overlap > old + 0.005) return `Mốc này tạo thêm phần chồng với câu ${j + 1}.`;
  }
  return "";
}

export function movedTimes(start: number, end: number, delta: number, mode: "move" | "start" | "end", duration: number) {
  const ms = (n: number) => Math.round(n * 1000) / 1000;
  if (mode === "start") return { start: ms(Math.max(0, Math.min(end - 0.08, start + delta))), end };
  if (mode === "end") return { start, end: ms(Math.min(duration, Math.max(start + 0.08, end + delta))) };
  const shift = Math.max(-start, Math.min(duration - end, delta));
  return { start: ms(start + shift), end: ms(end + shift) };
}

export function loopRange(start: number, end: number, duration: number) {
  return { start: Math.max(0, start - 0.25), end: Math.min(duration, end + 0.25) };
}
