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

export function lineWarning(line: ScriptLine, captionsOnly = false, audioOnly = false): string {
  const notes: string[] = [];
  if (line.source.trim() && !line.translation.trim()) {
    notes.push("Chưa có lời cho câu này");
  } else if (!captionsOnly && line.missing_audio) {
    notes.push("Thiếu file tiếng của câu");
  }
  if (!captionsOnly && !audioOnly) {
    const fit = fitWarning(line);
    if (fit) notes.push(fit);
  }
  if (!audioOnly) {
    const labels: Record<string, string> = {
      invalid_timing: "Mốc thời gian phụ đề không hợp lệ",
      overlap: "Mốc câu phụ đề chồng nhau",
      short_duration: "Phụ đề hiện dưới 0,65 giây, nên xem lại",
      dense_text: "Phụ đề nhiều chữ so với thời gian, nên xem lại",
    };
    for (const code of new Set(line.captions ?? [])) {
      if (labels[code]) notes.push(labels[code]);
    }
  }
  return notes.join(" · ");
}

export function nextFitWarning(indexes: number[], current: number): number | undefined {
  return indexes.find((index) => index > current) ?? indexes[0];
}
