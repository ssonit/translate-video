import { recoverSpeech } from "./recover.ts";
import type { Pcm16 } from "./piper";

export type SpeechRequest = {
  session: string; voice: string; lang: string; whole?: boolean;
  items: { index: number; text: string }[];
};

type Engine = {
  load: (lang: string, voice: string) => Promise<void>;
  speak: (text: string, lang: string) => Promise<Pcm16>;
  speakPassage: (text: string, lang: string) => Promise<Pcm16>;
  stop: () => void;
};

function errorText(caught: unknown) {
  if (caught instanceof Error) return caught.message;
  return typeof caught === "string" ? caught : "Không đọc được giọng.";
}

// Session ownership extends across await boundaries, including IPC and worker reloads.
export function bridgeRequests(
  engine: Engine,
  save: (session: string, index: number, pcm: Pcm16) => Promise<void>,
  finish: (session: string, error: string | null) => Promise<void>,
) {
  let generation = 0;
  let active: string | null = null;

  async function handle(request: SpeechRequest) {
    if (active !== null) {
      await finish(request.session, "Đang đọc một yêu cầu khác.");
      return;
    }
    active = request.session;
    const mine = ++generation;
    const current = () => generation === mine;
    let error: string | null = null;
    try {
      await engine.load(request.lang, request.voice);
      for (const item of request.items) {
        if (!current()) throw new Error("Đã dừng.");
        const read = () => request.whole
          ? engine.speakPassage(item.text, request.lang)
          : engine.speak(item.text, request.lang);
        const short = !request.whole && item.text.length <= (request.lang === "vi" ? 300 : 160);
        const pcm = short ? await recoverSpeech(read, async () => {
          if (!current()) throw new Error("Đã dừng.");
          engine.stop();
          await engine.load(request.lang, request.voice);
        }, () => !current()) : await read();
        if (!current()) throw new Error("Đã dừng.");
        await save(request.session, item.index, pcm);
      }
    } catch (caught) {
      error = errorText(caught);
    } finally {
      if (current()) active = null;
    }
    await finish(request.session, error);
  }

  function cancel(session?: string | null) {
    if (session && session !== active) return;
    generation += 1;
    active = null;
    engine.stop();
  }

  return { handle, cancel };
}
