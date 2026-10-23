import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { TtsEngine, type VoiceFiles } from "./engine";
import type { VoiceConfig } from "./piper";
import { prepareLine } from "./prepare";

type Item = { index: number; text: string };
type Request = { voice: string; lang: string; whole?: boolean; items: Item[] };

async function fetchVoice(lang: string, name: string): Promise<VoiceFiles> {
  const [model, config] = await Promise.all([
    invoke<ArrayBuffer>("read_voice", { lang, name, kind: "model" }),
    invoke<ArrayBuffer>("read_voice", { lang, name, kind: "config" }),
  ]);
  return { model, config: JSON.parse(new TextDecoder().decode(config)) as VoiceConfig };
}

async function save(index: number, rate: number, samples: Int16Array) {
  const bytes = new Uint8Array(samples.buffer, samples.byteOffset, samples.byteLength);
  await invoke("save_line_wav", bytes, { headers: { index: String(index), rate: String(rate) } });
}

function errorText(caught: unknown) {
  if (caught instanceof Error) return caught.message;
  return typeof caught === "string" ? caught : "Không đọc được giọng.";
}

// Lives for the whole app. Rust asks for the lines to be read; this speaks them with the
// Piper voice in voices/<language> and hands every wav back to Rust to write into the job folder.
export function useTtsBridge() {
  useEffect(() => {
    const engine = new TtsEngine(fetchVoice);
    let busy = false;

    async function handle(request: Request) {
      if (busy) {
        await invoke("tts_finish", { error: "Đang đọc một yêu cầu khác." });
        return;
      }
      busy = true;
      let error: string | null = null;
      try {
        await engine.load(request.lang, request.voice);
        for (const item of request.items) {
          const pcm = request.whole
            ? await engine.speakPassage(item.text, request.lang)
            : await engine.speak(item.text, request.lang);
          await save(item.index, pcm.rate, pcm.samples);
        }
      } catch (caught) {
        error = errorText(caught);
      } finally {
        busy = false;
      }
      await invoke("tts_finish", { error });
    }

    const warm = listen<{ lang: string; voice: string }>("tts-warm", (event) => {
      void engine.load(event.payload.lang, event.payload.voice).catch(() => {});
    });
    const request = listen<Request>("tts-request", (event) => void handle(event.payload));
    const cancel = listen("tts-cancel", () => engine.stop());
    const clean = listen<{ lang: string; items: { index: number; text: string }[] }>("tts-clean", (event) => {
      void (async () => {
        try {
          const lines = [];
          for (const item of event.payload.items) {
            lines.push({ index: item.index, chunks: await prepareLine(item.text, event.payload.lang) });
          }
          await invoke("tts_prepared", { lines });
        } catch (caught) {
          await invoke("tts_prepared", { lines: [], error: errorText(caught) });
        }
      })();
    });
    return () => {
      void warm.then((stop) => stop());
      void request.then((stop) => stop());
      void cancel.then((stop) => stop());
      void clean.then((stop) => stop());
      engine.stop();
    };
  }, []);
}
