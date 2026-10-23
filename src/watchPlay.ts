export const HOLD_GAP_SECS = 1;
export const DUCK_VOLUME = 0.22;

export type Span = { start: number; end: number };

export type Frame = {
  hold: boolean;
  volume: number;
  line: number | null;
  time: number;
};

export function frame(
  current: number,
  readyUntil: number,
  running: boolean,
  lines: Span[],
  duck = true,
): Frame {
  const time = running && current > readyUntil ? readyUntil : current;
  const hold = running && readyUntil - time < HOLD_GAP_SECS;
  let line: number | null = null;
  for (let index = 0; index < lines.length; index += 1) {
    const item = lines[index];
    if (time >= item.start && time < item.end) line = index;
  }
  const volume = duck && !hold && line !== null ? DUCK_VOLUME : 1;
  return { hold, volume, line, time };
}

export type MediaHead = {
  currentTime: number;
  duration: number;
  ended: boolean;
};

export type AudioMove = "start" | "seek" | "keep" | "stop";
export type VoiceGate = "play" | "finish" | "done";

export function audioMove(time: number, lineStart: number, lineChanged: boolean, head: MediaHead): AudioMove {
  if (lineChanged) return "start";
  const target = Math.max(0, time - lineStart);
  const known = Number.isFinite(head.duration) && head.duration > 0;
  if (head.ended || (known && target >= head.duration - 0.05)) return "stop";
  if (Math.abs(time - (lineStart + head.currentTime)) > 0.3) return "seek";
  return "keep";
}

// The clip still has words left when the picture is about to enter the next line.
// Finish means: hold the picture and let this clip play out. Done means the words are finished.
export function voiceGate(time: number, lineEnd: number, head: MediaHead, finishing: boolean): VoiceGate {
  const known = Number.isFinite(head.duration) && head.duration > 0 && Number.isFinite(head.currentTime);
  if (!known || head.ended) return finishing ? "done" : "play";
  const left = head.duration - head.currentTime;
  if (finishing) return left > 0.08 ? "finish" : "done";
  const slotLeft = lineEnd - time;
  if (slotLeft < 0.15 && left > slotLeft + 0.12) return "finish";
  return "play";
}

export function playerErrorDetail(): string {
  return "Chưa có hình để xem trước. Bản chỉ audio nghe được khi chạy xong.";
}

export function wantsPlay(hold: boolean, userPaused: boolean, started: boolean): boolean {
  return started && !hold && !userPaused;
}

// Đồng hồ khi chưa có file tiếng. Đang chạy thì không vượt mốc đã dịch.
export function advanceClock(
  current: number,
  delta: number,
  readyUntil: number,
  running: boolean,
  playing: boolean,
): number {
  if (!playing || !(delta > 0)) return current;
  const next = current + delta;
  if (running) return Math.min(next, readyUntil);
  return next;
}

export function listenErrorDetail(): string {
  return "Không phát được tiếng này.";
}

// Còn màn nghe thử / xem thử để quay lại. Tin tiến độ không được tự mở màn đó.
export function previewStillOpen(update: { running: boolean; published: boolean; lines: readonly unknown[] }): boolean {
  return update.running || update.published || update.lines.length > 0;
}
