import * as ort from "onnxruntime-web/wasm";
import mjsUrl from "../../node_modules/onnxruntime-web/dist/ort-wasm-simd-threaded.mjs?url";
import wasmUrl from "../../node_modules/onnxruntime-web/dist/ort-wasm-simd-threaded.wasm?url";
import { phonemize } from "phonemizer";
import { chunkText, processTextForTTS } from "./vendor/text-cleaner.js";
import { plainChunks, plainPassage } from "./prepare";
import { endMark, mergePhonemes } from "./phonemes";

export { mergePhonemes };

export type VoiceConfig = {
  audio: { sample_rate: number };
  phoneme_type?: string;
  espeak?: { voice?: string };
  phoneme_id_map: Record<string, number[]>;
  num_speakers: number;
  inference?: { noise_scale?: number; length_scale?: number; noise_w?: number };
};

export type Pcm16 = { rate: number; samples: Int16Array };

const SILENCE_SECS = 0.1;

export function idsFromSymbols(symbols: string[], map: Record<string, number[]>): number[] {
  const pad = map["_"] ?? [];
  const ids: number[] = [...(map["^"] ?? []), ...pad];
  for (const symbol of symbols) {
    const id = map[symbol];
    if (id) ids.push(...id, ...pad);
  }
  ids.push(...(map["$"] ?? []));
  return ids;
}

export function toPcm16(audio: Float32Array, rate: number): Pcm16 {
  let peak = 1e-9;
  for (let i = 0; i < audio.length; i += 1) peak = Math.max(peak, Math.abs(audio[i]));
  const gain = Math.min(1, 1 / peak);
  const samples = new Int16Array(audio.length);
  for (let i = 0; i < audio.length; i += 1) {
    const value = Math.max(-1, Math.min(1, audio[i] * gain));
    samples[i] = value < 0 ? Math.round(value * 0x8000) : Math.round(value * 0x7fff);
  }
  return { rate, samples };
}

export class Piper {
  private constructor(
    private readonly session: ort.InferenceSession,
    private readonly config: VoiceConfig,
  ) {}

  static async load(model: ArrayBuffer, config: VoiceConfig): Promise<Piper> {
    ort.env.wasm.wasmPaths = { mjs: mjsUrl, wasm: wasmUrl };
    // One thread: WebView2 only offers more with cross-origin isolation, which the app does not set.
    ort.env.wasm.numThreads = 1;
    const session = await ort.InferenceSession.create(new Uint8Array(model), {
      executionProviders: ["wasm"],
    });
    return new Piper(session, config);
  }

  private async phonemeIds(text: string, endPause: boolean): Promise<number[]> {
    let symbols: string[];
    if (this.config.phoneme_type === "text") {
      symbols = Array.from(text.normalize("NFD"));
    } else {
      const parts = await phonemize(text, this.config.espeak?.voice ?? "en-us");
      let merged = mergePhonemes(text, parts).replace(/\(en\)/g, "").replace(/\(vi\)/g, "").trim();
      if (endPause) merged = endMark(merged);
      symbols = Array.from(merged.normalize("NFD"));
    }
    return idsFromSymbols(symbols, this.config.phoneme_id_map);
  }

  private async speakChunk(text: string, endPause = false): Promise<Float32Array> {
    const ids = await this.phonemeIds(text, endPause);
    const inference = this.config.inference ?? {};
    const feeds: Record<string, ort.Tensor> = {
      input: new ort.Tensor("int64", BigInt64Array.from(ids, (id) => BigInt(id)), [1, ids.length]),
      input_lengths: new ort.Tensor("int64", BigInt64Array.from([BigInt(ids.length)]), [1]),
      scales: new ort.Tensor(
        "float32",
        Float32Array.from([inference.noise_scale ?? 0.667, inference.length_scale ?? 1, inference.noise_w ?? 0.8]),
        [3],
      ),
    };
    if (this.config.num_speakers > 1) {
      feeds.sid = new ort.Tensor("int64", BigInt64Array.from([0n]), [1]);
    }
    const results = await this.session.run(feeds);
    return new Float32Array(results.output.data as Float32Array);
  }

  async pieces(raw: string, lang: string): Promise<string[]> {
    if (lang === "vi") {
      return (await chunkText(await processTextForTTS(raw))).filter((chunk) => chunk.trim() !== "");
    }
    return plainChunks(raw);
  }

  async piece(text: string): Promise<Pcm16> {
    return toPcm16(await this.speakChunk(text, true), this.config.audio.sample_rate);
  }

  async speak(raw: string, lang: string): Promise<Pcm16> {
    const rate = this.config.audio.sample_rate;
    const chunks = await this.pieces(raw, lang);
    if (chunks.length === 0) {
      return { rate, samples: new Int16Array(Math.round(rate * SILENCE_SECS)) };
    }
    const parts: Float32Array[] = [];
    for (const chunk of chunks) parts.push(await this.speakChunk(chunk));
    const audio = new Float32Array(parts.reduce((sum, part) => sum + part.length, 0));
    let offset = 0;
    for (const part of parts) {
      audio.set(part, offset);
      offset += part.length;
    }
    return toPcm16(audio, rate);
  }

  // One model pass for a whole passage. Sentence marks stay inside the phoneme string.
  async speakPassage(raw: string, lang: string): Promise<Pcm16> {
    const rate = this.config.audio.sample_rate;
    const text = lang === "vi" ? await processTextForTTS(raw) : plainPassage(raw);
    const spoken = text.trim();
    if (!spoken) {
      return { rate, samples: new Int16Array(Math.round(rate * SILENCE_SECS)) };
    }
    return toPcm16(await this.speakChunk(spoken, true), rate);
  }
}
