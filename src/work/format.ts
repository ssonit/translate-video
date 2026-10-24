import type { DraftBrief, FinishedDub, KeptVersion, LinePick, NameFix, ScriptLine, StepView } from "./types";

// weight: rough share of a whole run, used to blend the steps into one overall estimate.
const STEPS = [
  { id: "extract", label: "Tách tiếng", weight: 3 },
  { id: "transcribe", label: "Chép lời", weight: 25 },
  { id: "split", label: "Ngắt câu", weight: 6 },
  { id: "names", label: "Tên", weight: 2 },
  { id: "terms", label: "Thuật ngữ", weight: 2 },
  { id: "translate", label: "Dịch", weight: 14 },
  { id: "speak", label: "Đọc giọng", weight: 35 },
  { id: "fit", label: "Co độ dài", weight: 8 },
  { id: "merge", label: "Ghép video", weight: 7 },
];

// Ngôn ngữ AI sẽ dịch sang (mã khớp TARGETS trong pipeline). Ngôn ngữ trong video luôn do Whisper tự nhận.
export const LANGUAGES = [
  { code: "vi", name: "Tiếng Việt" },
  { code: "en", name: "Tiếng Anh" },
  { code: "zh", name: "Tiếng Trung" },
  { code: "ja", name: "Tiếng Nhật" },
  { code: "ko", name: "Tiếng Hàn" },
  { code: "fr", name: "Tiếng Pháp" },
  { code: "de", name: "Tiếng Đức" },
  { code: "es", name: "Tiếng Tây Ban Nha" },
  { code: "ru", name: "Tiếng Nga" },
  { code: "th", name: "Tiếng Thái" },
  { code: "id", name: "Tiếng Indonesia" },
  { code: "pt", name: "Tiếng Bồ Đào Nha" },
];

export function pickedEdits(lines: ScriptLine[], picks: Record<number, LinePick>) {
  const edits: { index: number; text: string }[] = [];
  for (const [key, pick] of Object.entries(picks)) {
    const index = Number(key);
    const line = lines[index];
    if (!line) continue;
    const text = pick.text.trim();
    if (text === line.translation.trim() && !pick.again) continue;
    edits.push({ index, text });
  }
  edits.sort((left, right) => left.index - right.index);
  return edits;
}

export function freshSteps(audioOnly = false, textOnly = false): StepView[] {
  return STEPS.map((step) => ({
    ...step,
    label:
      textOnly && step.id === "merge"
        ? "Ghi chữ"
        : audioOnly && step.id === "merge"
          ? "Xuất audio"
          : audioOnly && step.id === "fit"
            ? "Nối câu"
            : step.label,
    state: textOnly && (step.id === "speak" || step.id === "fit") ? "skipped" : "pending",
    detail: "",
    progress: null,
    startedAt: null,
  }));
}

export function stepFraction(step: StepView) {
  if (step.state === "done") return 1;
  if (step.state === "running") return step.progress ?? 0;
  return 0;
}

// Remaining time of one step, from how fast it has moved so far. Null until the rate is believable.
export function stepRemaining(step: StepView, now: number) {
  if (step.state !== "running" || step.progress === null || step.startedAt === null) return null;
  const elapsed = (now - step.startedAt) / 1000;
  if (step.progress < 0.03 || elapsed < 3) return null;
  return (elapsed * (1 - step.progress)) / step.progress;
}

export function overallEstimate(steps: StepView[], startedAt: number | null, now: number) {
  const active = steps.filter((step) => step.state !== "skipped");
  const total = active.reduce((sum, step) => sum + step.weight, 0);
  const done = active.reduce((sum, step) => sum + step.weight * stepFraction(step), 0);
  const fraction = total > 0 ? done / total : 0;
  const elapsed = startedAt === null ? 0 : (now - startedAt) / 1000;
  const remaining = done > 0 && elapsed >= 5 ? ((total - done) * elapsed) / done : null;
  return { fraction, elapsed, remaining };
}

export function duration(seconds: number) {
  const secs = Math.max(0, Math.round(seconds));
  if (secs < 60) return `${secs} giây`;
  const minutes = Math.round(secs / 60);
  if (minutes < 60) return `${minutes} phút`;
  const rest = minutes % 60;
  return rest === 0 ? `${Math.floor(minutes / 60)} giờ` : `${Math.floor(minutes / 60)} giờ ${rest} phút`;
}

export function remainingText(seconds: number | null) {
  if (seconds === null) return "";
  return seconds < 10 ? "sắp xong" : `còn khoảng ${duration(seconds)}`;
}

export function fileName(path: string) {
  const parts = path.split(/[/\\]/);
  return parts[parts.length - 1] || path;
}

export function isCaptioned(path: string) {
  return /\.chu\.mp4$/i.test(path);
}

// Bản chỉ chữ là file .chu mà thư viện liệt kê, hoặc file vừa chạy xong khi danh sách chưa kịp đổi.
// Bản lồng tiếng đang xem file chữ đốt kèm thì libraryPath vẫn là mp4 thường và marked là false.
export function showAsTextOnly(result: string, libraryPath = "", marked = false) {
  if (isAudio(result)) return false;
  if (isCaptioned(libraryPath)) return true;
  if (marked && isCaptioned(result)) return true;
  if (!libraryPath && isCaptioned(result)) return true;
  return false;
}

export function watchHeading(result: string, libraryPath = "", marked = false) {
  if (showAsTextOnly(result, libraryPath, marked)) return "Video chỉ có chữ";
  if (isCaptioned(result)) return "Video đã lồng, có chữ";
  return "Video đã lồng";
}

export function isAudio(path: string) {
  return /\.m4a$/i.test(path);
}

export function isListen(path: string) {
  return /\.nghe\.m4a$/i.test(path);
}

export function plainVideo(path: string) {
  return path.replace(/\.chu\.mp4$/i, ".mp4");
}

export function captionedVideo(path: string) {
  return plainVideo(path).replace(/\.mp4$/i, ".chu.mp4");
}

// File video mà nút Tải video lưu: bản đã cất đang xem nếu có, không thì kết quả. Đang xem audio thì không có video để lưu.
export function videoToSave(result: string, playingKept = "") {
  const shown = playingKept || result;
  return /\.mp4$/i.test(shown) ? shown : "";
}

export function sameFile(left: string, right: string) {
  const norm = (value: string) => value.replace(/\//g, "\\").replace(/\\+$/, "").toLowerCase();
  return norm(left) === norm(right);
}

export function languageName(code: string) {
  return LANGUAGES.find((item) => item.code === code)?.name ?? code;
}

export function fold(value: string) {
  return value
    .toLocaleLowerCase("vi")
    .normalize("NFD")
    .replace(/\p{M}/gu, "")
    .replace(/đ/g, "d");
}

export function keptLabel(item: KeptVersion) {
  const date = new Date(item.modifiedMs);
  const day = String(date.getDate()).padStart(2, "0");
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const hour = String(date.getHours()).padStart(2, "0");
  const minute = String(date.getMinutes()).padStart(2, "0");
  const name = item.language ? languageName(item.language) : "Bản cũ";
  return `${name} · ${day}/${month} ${hour}:${minute}`;
}

export function againPlan(input: {
  retranscribe: boolean;
  lang: string;
  currentLang: string;
  voice: string;
  savedVoice: string;
  names: NameFix[];
  base: NameFix[];
  textOnly: boolean;
  wasTextOnly: boolean;
}) {
  if (input.retranscribe) return "Chép lại lời từ đầu. Video đang xem vẫn mở được sau đó.";
  const langChanged = input.lang !== input.currentLang;
  const voiceChanged = input.voice !== "" && input.savedVoice !== "" && input.voice !== input.savedVoice;
  const namesChanged = nameSignature(input.names) !== nameSignature(input.base);
  const modeChanged = input.textOnly !== input.wasTextOnly;
  if (langChanged) {
    return input.textOnly
      ? `Dịch lại sang ${languageName(input.lang)} và ghi chữ lên hình. Không đọc giọng.`
      : `Dịch và đọc lại sang ${languageName(input.lang)}. Lời chép giữ. Video đang xem vẫn mở được.`;
  }
  if (modeChanged) {
    return input.textOnly
      ? "Giữ tiếng gốc, ghi chữ dịch lên hình. Không đọc giọng mới."
      : "Đọc giọng và ghép lại video. Bản chỉ chữ được cất.";
  }
  if (namesChanged && voiceChanged && !input.textOnly) return "Dịch lại câu có tên đổi, rồi đọc lại bằng giọng mới. Lời chép giữ.";
  if (namesChanged && input.textOnly) return "Dịch lại những câu có chữ đổi, rồi ghi chữ lên hình.";
  if (namesChanged) return "Dịch và đọc lại những câu có chữ đổi. Câu khác giữ nguyên.";
  if (voiceChanged && !input.textOnly) return "Đọc lại bằng giọng mới. Bản dịch giữ.";
  return "Chưa đổi ngôn ngữ, giọng hay tên.";
}

export function nameSignature(rows: NameFix[]) {
  return rows
    .map((row) => ({ from: row.from.trim(), to: row.to.trim() }))
    .filter((row) => [...row.from].filter((ch) => /\p{L}/u.test(ch)).length >= 2 && row.to && row.from.toLocaleLowerCase() !== row.to.toLocaleLowerCase())
    .map((row) => `${row.from.toLocaleLowerCase()}\n${row.to}`)
    .sort()
    .join("\n");
}

export function when(ms: number) {
  if (!ms) return "";
  return new Date(ms).toLocaleString("vi-VN", {
    day: "2-digit",
    month: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

// Việc dở đã làm tới đâu, từ số câu đã chép, đã dịch và đã đọc.
export function draftProgress(brief: DraftBrief) {
  const { heard, translated, spoken } = brief;
  if (heard === 0) return "Chưa chép lời.";
  if (translated === 0) return `Đã chép ${heard} câu, chưa dịch.`;
  if (translated < heard) return `Đã chép ${heard} câu, dịch được ${translated}.`;
  if (brief.textOnly) return `Đã dịch ${heard} câu, chưa ghi chữ vào video.`;
  if (spoken === 0) return `Đã dịch ${heard} câu, chưa đọc giọng.`;
  if (spoken < heard) return `Đã dịch ${heard} câu, mới đọc ${spoken}/${heard} câu.`;
  return `Đã dịch và đọc ${heard} câu, chưa ghép video.`;
}

export function parentFolder(path: string) {
  return path.replace(/[/\\][^/\\]*$/, "");
}

export function jobFolder(item: FinishedDub) {
  return item.done ? parentFolder(item.path) : item.path;
}

export function openJobFolder(path: string) {
  if (!path) return "";
  return parentFolder(isAudio(path) ? path : plainVideo(path));
}

export function errorText(caught: unknown) {
  if (typeof caught === "string") return caught;
  if (caught && typeof caught === "object" && "message" in caught && typeof caught.message === "string") {
    return caught.message;
  }
  return "Không lồng được video.";
}

export function nameRowError(rows: NameFix[]) {
  for (const row of rows) {
    const from = row.from.trim();
    const to = row.to.trim();
    if (!from && !to) continue;
    const letters = [...from].filter((ch) => /\p{L}/u.test(ch)).length;
    if (letters < 2) return "Ô Nghe được cần ít nhất 2 chữ cái.";
    if (!to) return "Điền tên đúng, hoặc xóa dòng đó.";
    if (from.toLocaleLowerCase() === to.toLocaleLowerCase()) return "Hai ô đang giống nhau.";
  }
  return "";
}

export function mark(state: string) {
  if (state === "done" || state === "skipped") return "mark is-ready";
  if (state === "running") return "mark is-run";
  if (state === "error") return "mark is-bad";
  return "mark is-wait";
}

export function markText(state: string) {
  if (state === "done") return "Xong";
  if (state === "skipped") return "Bỏ qua";
  if (state === "running") return "Đang chạy";
  if (state === "error") return "Lỗi";
  return "Chờ";
}
