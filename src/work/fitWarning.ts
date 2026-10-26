import type { ScriptLine } from "./types";

const number = new Intl.NumberFormat("vi-VN", { maximumFractionDigits: 2 });
const speedNumber = new Intl.NumberFormat("vi-VN", { maximumFractionDigits: 3 });

export function fitWarning(line: ScriptLine): string {
  if (!line.fit) return "";
  const { cut_seconds: cut, speed } = line.fit;
  const notes: string[] = [];
  if (Number.isFinite(cut) && cut > 0.02) {
    notes.push(`Bị cắt ${number.format(cut)} giây cuối`);
  }
  if (speed !== null && Number.isFinite(speed) && speed > 1.001) {
    notes.push(`Đã tăng tốc ${speedNumber.format(speed)}×`);
  }
  return notes.join(" · ");
}

export function nextFitWarning(indexes: number[], current: number): number | undefined {
  return indexes.find((index) => index > current) ?? indexes[0];
}
