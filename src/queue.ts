// Hàng đợi chỉ xếp việc. Một video chạy xong mới tới video sau.
// Danh sách chờ ghi vào máy, thoát app rồi mở lại vẫn còn.

import { shareLink } from "./shareLink.ts";

const QUEUE_STORAGE_KEY = "dub-queue";

export type QueueJob = {
  id: string;
  label: string;
  mode: "file" | "link";
  source: string;
  folder: string;
  target: string;
  voice: string;
  audioOnly: boolean;
  skipTranslate: boolean;
  burnCaptions: boolean;
  textOnly: boolean;
};

// Đoạn chia sẻ Douyin hoặc TikTok thì lấy link trong đó. Chữ khác giữ nguyên.
function linkIn(source: string) {
  const value = source.trim();
  return shareLink(value) ?? value;
}

export function queueKey(mode: "file" | "link", source: string) {
  const value = source.trim();
  if (mode === "link") return `link:${linkIn(value)}`;
  return `file:${value.replace(/\//g, "\\").replace(/\\+$/, "").toLowerCase()}`;
}

export function queueLabel(mode: "file" | "link", source: string) {
  const value = source.trim();
  if (mode === "link") {
    const link = linkIn(value);
    try {
      const url = new URL(link);
      const host = url.hostname.replace(/^www\./, "");
      const fromQuery = url.searchParams.get("v") || url.searchParams.get("modal_id");
      const id = fromQuery || url.pathname.split("/").filter(Boolean).pop();
      return id ? `${host} · ${id}` : host;
    } catch {
      return link.length > 48 ? `${link.slice(0, 48)}…` : link;
    }
  }
  const parts = value.split(/[/\\]/);
  return parts[parts.length - 1] || value;
}

// Bỏ video đang chạy và video đã có trong hàng. Giữ thứ tự thêm vào.
export function appendJobs(queue: QueueJob[], incoming: QueueJob[], activeKey = "") {
  const seen = new Set(queue.map((item) => queueKey(item.mode, item.source)));
  if (activeKey) seen.add(activeKey);
  const added: QueueJob[] = [];
  let skipped = 0;
  for (const item of incoming) {
    const key = queueKey(item.mode, item.source);
    if (!item.source.trim() || seen.has(key)) {
      skipped += 1;
      continue;
    }
    seen.add(key);
    added.push(item);
  }
  return { queue: [...queue, ...added], added, skipped };
}

export type SavedQueue = {
  waiting: QueueJob[];
  current: QueueJob | null;
};

const EMPTY_QUEUE: SavedQueue = { waiting: [], current: null };

function isQueueJob(value: unknown): value is QueueJob {
  if (!value || typeof value !== "object") return false;
  const item = value as Partial<QueueJob>;
  return (
    typeof item.id === "string" &&
    item.id !== "" &&
    typeof item.label === "string" &&
    (item.mode === "file" || item.mode === "link") &&
    typeof item.source === "string" &&
    item.source.trim() !== "" &&
    typeof item.folder === "string" &&
    typeof item.target === "string" &&
    typeof item.voice === "string" &&
    typeof item.audioOnly === "boolean" &&
    typeof item.skipTranslate === "boolean" &&
    typeof item.burnCaptions === "boolean"
  );
}

function asQueueJob(value: unknown): QueueJob | null {
  if (!isQueueJob(value)) return null;
  const item = value as QueueJob & { textOnly?: unknown };
  return { ...item, textOnly: item.textOnly === true };
}

export function parseSavedQueue(raw: string | null): SavedQueue {
  if (!raw) return EMPTY_QUEUE;
  try {
    const parsed = JSON.parse(raw) as { waiting?: unknown; current?: unknown };
    const waiting = Array.isArray(parsed.waiting)
      ? parsed.waiting.flatMap((item) => {
          const job = asQueueJob(item);
          return job ? [job] : [];
        })
      : [];
    const current = asQueueJob(parsed.current);
    return { waiting, current };
  } catch {
    return EMPTY_QUEUE;
  }
}

export function readSavedQueue(): SavedQueue {
  try {
    if (typeof localStorage === "undefined") return EMPTY_QUEUE;
    return parseSavedQueue(localStorage.getItem(QUEUE_STORAGE_KEY));
  } catch {
    return EMPTY_QUEUE;
  }
}

export function writeSavedQueue(saved: SavedQueue) {
  try {
    if (typeof localStorage === "undefined") return;
    localStorage.setItem(QUEUE_STORAGE_KEY, JSON.stringify(saved));
  } catch {
    // Hàng vẫn chạy trong phiên này khi máy không ghi được.
  }
}

// Video đang chạy dở được xếp lại lên đầu, trừ khi tiến trình đó vẫn còn chạy.
export function restoreQueue(saved: SavedQueue, stillRunning: boolean): QueueJob[] {
  if (stillRunning || !saved.current) return saved.waiting;
  const key = queueKey(saved.current.mode, saved.current.source);
  const rest = saved.waiting.filter((item) => queueKey(item.mode, item.source) !== key);
  return [saved.current, ...rest];
}

// Gỡ video sắp chạy khỏi hàng chờ. Video chạy dở trước đó, nếu khác video này, được trả lại lên đầu.
export function takeActive(waiting: QueueJob[], active: QueueJob | null, job: QueueJob) {
  const jobKey = queueKey(job.mode, job.source);
  let next = waiting.filter((item) => queueKey(item.mode, item.source) !== jobKey);
  if (active) {
    const prevKey = queueKey(active.mode, active.source);
    if (prevKey !== jobKey && !next.some((item) => queueKey(item.mode, item.source) === prevKey)) {
      next = [active, ...next];
    }
  }
  return { waiting: next, active: job };
}
