import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { TtsEngine, type VoiceFiles } from "./engine";
import type { VoiceConfig } from "./piper";
import { prepareLine } from "./prepare";
import { bridgeRequests, type SpeechRequest } from "./bridgeRequests";

async function fetchVoice(lang: string, name: string): Promise<VoiceFiles> {
  const [model, config] = await Promise.all([
    invoke<ArrayBuffer>("read_voice", { lang, name, kind: "model" }),
    invoke<ArrayBuffer>("read_voice", { lang, name, kind: "config" }),
  ]);
  return { model, config: JSON.parse(new TextDecoder().decode(config)) as VoiceConfig };
}

async function save(session: string, index: number, rate: number, samples: Int16Array) {
  const bytes = new Uint8Array(samples.buffer, samples.byteOffset, samples.byteLength);
  await invoke("save_line_wav", bytes, { headers: { session, index: String(index), rate: String(rate) } });
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
    let generation = 0;
    const requests = bridgeRequests(engine,
      (session, index, pcm) => save(session, index, pcm.rate, pcm.samples),
      async (session, error) => { await invoke("tts_finish", { session, error }); },
    );

    const warm = listen<{ lang: string; voice: string }>("tts-warm", (event) => {
      void engine.load(event.payload.lang, event.payload.voice).catch(() => {});
    });
    const request = listen<SpeechRequest>("tts-request", (event) => void requests.handle(event.payload).catch(console.error));
    const cancel = listen<string | null>("tts-cancel", (event) => {
      if (!event.payload) generation += 1;
      requests.cancel(event.payload);
    });
    const clean = listen<{ session: string; lang: string; items: { index: number; text: string }[] }>("tts-clean", (event) => {
      const mine = generation;
      const session = event.payload.session;
      void (async () => {
        try {
          const lines = [];
          for (const item of event.payload.items) {
            if (mine !== generation) return;
            lines.push({ index: item.index, chunks: await prepareLine(item.text, event.payload.lang) });
          }
          if (mine !== generation) return;
          await invoke("tts_prepared", { session, lines });
        } catch (caught) {
          await invoke("tts_prepared", { session, lines: [], error: errorText(caught) });
        }
      })().catch(console.error);
    });
    return () => {
      generation += 1;
      void warm.then((stop) => stop());
      void request.then((stop) => stop());
      void cancel.then((stop) => stop());
      void clean.then((stop) => stop());
      requests.cancel();
    };
  }, []);
}
