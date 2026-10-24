import type { Pcm16 } from "./piper";
import type { FromWorker, ToWorker } from "./tts.worker";
import type { VoiceConfig } from "./piper";
import { loadJoin, loadStillCurrent, slotAfterCrash } from "./streamGate";

export type VoiceFiles = { model: ArrayBuffer; config: VoiceConfig };

type Waiter<T> = { resolve: (value: T) => void; reject: (error: Error) => void };

type StreamWaiter = {
  onChunk: (pcm: Pcm16) => void;
  resolve: () => void;
  reject: (error: Error) => void;
};

export const STOPPED = "Đã dừng.";

// Runs Piper voices in a Web Worker so the screen stays responsive. One voice at a time.
export class TtsEngine {
  private worker: Worker | null = null;
  private voice = "";
  private loadingKey = "";
  private loadingPromise: Promise<void> | null = null;
  private epoch = 0;
  private nextId = 1;
  private loading: Waiter<void> | null = null;
  private pending = new Map<number, Waiter<Pcm16>>();
  private streams = new Map<number, StreamWaiter>();

  constructor(private readonly fetchVoice: (lang: string, name: string) => Promise<VoiceFiles>) {}

  private start(): Worker {
    const worker = new Worker(new URL("./tts.worker.ts", import.meta.url), { type: "module" });
    worker.onmessage = (event: MessageEvent<FromWorker>) => this.receive(event.data);
    worker.onerror = (event) => {
      if (this.worker !== worker) return;
      const crashed = slotAfterCrash({ voice: this.voice });
      this.worker = crashed.alive ? worker : null;
      this.voice = crashed.voice;
      this.fail(new Error(event.message || "Bộ đọc giọng bị lỗi."));
      worker.terminate();
    };
    this.worker = worker;
    return worker;
  }

  private receive(message: FromWorker) {
    if (message.type === "ready") {
      this.loading?.resolve();
      this.loading = null;
    } else if (message.type === "pcm") {
      this.pending.get(message.id)?.resolve({ rate: message.rate, samples: message.samples });
      this.pending.delete(message.id);
    } else if (message.type === "chunk") {
      this.streams.get(message.id)?.onChunk({ rate: message.rate, samples: message.samples });
    } else if (message.type === "done") {
      this.streams.get(message.id)?.resolve();
      this.streams.delete(message.id);
    } else if (message.id === undefined) {
      this.loading?.reject(new Error(message.message));
      this.loading = null;
    } else {
      this.pending.get(message.id)?.reject(new Error(message.message));
      this.pending.delete(message.id);
      this.streams.get(message.id)?.reject(new Error(message.message));
      this.streams.delete(message.id);
    }
  }

  private fail(error: Error) {
    this.loading?.reject(error);
    this.loading = null;
    for (const waiter of this.pending.values()) waiter.reject(error);
    this.pending.clear();
    for (const waiter of this.streams.values()) waiter.reject(error);
    this.streams.clear();
  }

  // A voice name can repeat in two languages, so the loaded voice is told apart by language too.
  // A second call for the voice already loading waits on the same session instead of starting another.
  async load(lang: string, voice: string): Promise<void> {
    const key = `${lang}/${voice}`;
    const action = loadJoin(this.worker ? this.voice : "", this.loadingKey, key);
    if (action === "ready") return;
    if (action === "join" && this.loadingPromise) return this.loadingPromise;
    this.stop();
    this.loadingKey = key;
    const pending = this.beginLoad(lang, voice, key);
    this.loadingPromise = pending;
    try {
      await pending;
    } finally {
      if (this.loadingPromise === pending) {
        this.loadingPromise = null;
        this.loadingKey = "";
      }
    }
  }

  private async beginLoad(lang: string, voice: string, key: string): Promise<void> {
    const started = this.epoch;
    const files = await this.fetchVoice(lang, voice);
    if (!loadStillCurrent(started, this.epoch)) throw new Error(STOPPED);
    const worker = this.start();
    const ready = new Promise<void>((resolve, reject) => {
      this.loading = { resolve, reject };
    });
    const message: ToWorker = { type: "load", model: files.model, config: files.config };
    worker.postMessage(message, { transfer: [files.model] });
    await ready;
    if (!loadStillCurrent(started, this.epoch)) throw new Error(STOPPED);
    this.voice = key;
  }

  speak(text: string, lang: string): Promise<Pcm16> {
    const worker = this.worker;
    if (!worker) return Promise.reject(new Error("Giọng chưa được nạp."));
    const id = this.nextId++;
    return new Promise<Pcm16>((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      const message: ToWorker = { type: "speak", id, text, lang };
      worker.postMessage(message);
    });
  }

  // Drops the current utterance and keeps the loaded voice. The next listen does not read the model again.
  cancel() {
    this.worker?.postMessage({ type: "cancel" });
    const stopped = new Error(STOPPED);
    for (const waiter of this.streams.values()) waiter.reject(stopped);
    this.streams.clear();
    for (const waiter of this.pending.values()) waiter.reject(stopped);
    this.pending.clear();
  }

  // Each sentence is handed back as soon as it is spoken, so playback can start before the part ends.
  speakStream(text: string, lang: string, onChunk: (pcm: Pcm16) => void): Promise<void> {
    const worker = this.worker;
    if (!worker) return Promise.reject(new Error("Giọng chưa được nạp."));
    const id = this.nextId++;
    return new Promise<void>((resolve, reject) => {
      this.streams.set(id, { onChunk, resolve, reject });
      const message: ToWorker = { type: "speak", id, text, lang, stream: true };
      worker.postMessage(message);
    });
  }

  speakPassage(text: string, lang: string): Promise<Pcm16> {
    const worker = this.worker;
    if (!worker) return Promise.reject(new Error("Giọng chưa được nạp."));
    const id = this.nextId++;
    return new Promise<Pcm16>((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      const message: ToWorker = { type: "speak", id, text, lang, whole: true };
      worker.postMessage(message);
    });
  }

  // Ends the worker and fails whatever is waiting. The next load starts a fresh one.
  stop() {
    this.epoch += 1;
    this.fail(new Error(STOPPED));
    this.worker?.terminate();
    this.worker = null;
    this.voice = "";
  }
}
