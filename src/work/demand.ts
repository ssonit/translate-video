import type { RenderSettings } from "./renderSettings";

export type DraftOptions = {
  target: string; voice: string; provider: string; model: string;
  audioOnly: boolean; textOnly: boolean; burn: boolean; skipTranslate: boolean; useTerms: boolean; render: RenderSettings;
};
export type DraftCue = { id: string; slot: number; sources: number[]; authored: boolean; keep_speech: boolean;
  line: { start: number; end: number; source: string; translation: string } };
export type DraftView = { root: string; source: string; revision: string; duration: number; options: DraftOptions;
  cues: DraftCue[]; canUndo: boolean; namesReviewed: boolean; ready: string[]; lastExport: string | null; warning?: string | null };
export type DraftClip = { id: string; path: string; version: string; start: number; speed: number; played: number; tentative: boolean };
export type DraftRegion = { view: DraftView; clips: DraftClip[] };
export type RegionInput = { root: string; start: number; end: number; listen: boolean; cue?: string | null };
export type RegionIntent = RegionInput & { serial: string; playing: boolean };
export type DraftPatch = { id: string; source?: string; text?: string; start?: number; end?: number };

// Exactly one IPC operation plus one replaceable pending intent. Publishing the
// newest intent does not enqueue another worker while an old request drains.
export function regionQueue<T>(ports: {
  publish: (intent: RegionIntent | null) => Promise<unknown>;
  run: (root: string, serial: string) => Promise<T>;
  accept: (value: T) => void;
  fail: (error: unknown) => void;
  busy: (active: boolean) => void;
  cancel: (root: string, serial: string) => Promise<unknown>;
}, prefix: string) {
  let counter = 0, epoch = 0, disposed = false;
  let desired: RegionIntent | null = null;
  let pending: { intent: RegionIntent; published: Promise<unknown> } | null = null;
  let active: Promise<void> | null = null;
  let running: RegionIntent | null = null;
  let publishing: Promise<unknown> = Promise.resolve();
  const publish = (intent: RegionIntent | null) => {
    publishing = publishing.catch(() => {}).then(() => ports.publish(intent));
    return publishing;
  };
  function pump() {
    if (active || disposed || !pending) return;
    const mine = epoch;
    ports.busy(true);
    active = (async () => {
      while (pending && !disposed && mine === epoch) {
        const next = pending;
        pending = null;
        try {
          await next.published;
          if (disposed || mine !== epoch || desired?.serial !== next.intent.serial || !desired.playing) continue;
          running = next.intent;
          const result = await ports.run(next.intent.root, next.intent.serial);
          running = null;
          if (!disposed && mine === epoch && desired?.root === next.intent.root) ports.accept(result);
        } catch (error) {
          running = null;
          if (!disposed && mine === epoch) ports.fail(error);
        }
      }
    })().finally(() => {
      active = null;
      if (!disposed) ports.busy(false);
      if (pending && !disposed) pump();
    });
  }
  function request(input: RegionInput) {
    if (disposed) return;
    desired = { ...input, serial: `${prefix}:${++counter}`, playing: true };
    pending = { intent: desired, published: publish(desired) };
    pump();
  }
  async function pause() {
    pending = null;
    if (desired) desired = { ...desired, serial: `${prefix}:${++counter}`, playing: false };
    await publish(desired);
  }
  async function idle() { while (active) await active; await publishing.catch(() => {}); }
  async function close() {
    if (disposed) return;
    disposed = true; epoch += 1; pending = null; desired = null;
    const old = running;
    const released = publish(null);
    if (old) await ports.cancel(old.root, old.serial);
    await released.catch(() => {});
    await idle();
    ports.busy(false);
  }
  return { request, pause, idle, close };
}

export const LOOK_AHEAD_SECONDS = 30;

export function missingRegion(view: DraftView, start: number, listen: boolean): boolean {
  const end = Math.min(view.duration, start + LOOK_AHEAD_SECONDS);
  return view.cues.some(c => c.line.start < end && c.line.end > start
    && (!c.line.translation.trim() || (listen && !view.ready.includes(c.id))));
}

export function splitText(text: string, fraction: number): [string, string] {
  const words = text.trim().split(/\s+/u).filter(Boolean);
  if (words.length < 2) return [text.trim(), ""];
  const cut = Math.max(1, Math.min(words.length - 1, Math.round(words.length * fraction)));
  return [words.slice(0, cut).join(" "), words.slice(cut).join(" ")];
}
