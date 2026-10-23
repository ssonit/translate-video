import { Piper, type VoiceConfig } from "./piper";
import { listenPieces } from "./prepare";
import { enqueueSpeak } from "./streamGate";
import { chunkText, processTextForTTS } from "./vendor/text-cleaner.js";

export type ToWorker =
  | { type: "load"; model: ArrayBuffer; config: VoiceConfig }
  | { type: "cancel" }
  | { type: "speak"; id: number; text: string; lang: string; whole?: boolean; stream?: boolean };

export type FromWorker =
  | { type: "ready" }
  | { type: "failed"; id?: number; message: string }
  | { type: "chunk"; id: number; rate: number; samples: Int16Array }
  | { type: "done"; id: number }
  | { type: "pcm"; id: number; rate: number; samples: Int16Array };

let piper: Piper | null = null;
let run = 0;
let chain = Promise.resolve();

function send(message: FromWorker, transfer: Transferable[] = []) {
  self.postMessage(message, { transfer });
}

function messageOf(caught: unknown) {
  return caught instanceof Error ? caught.message : String(caught);
}

self.onmessage = (event: MessageEvent<ToWorker>) => {
  const message = event.data;
  if (message.type === "cancel") {
    run += 1;
    return;
  }
  chain = enqueueSpeak(chain, () => dispatch(message));
};

async function dispatch(message: ToWorker) {
  if (message.type === "cancel") return;
  if (message.type === "load") {
    try {
      piper = await Piper.load(message.model, message.config);
      send({ type: "ready" });
    } catch (caught) {
      send({ type: "failed", message: messageOf(caught) });
    }
    return;
  }
  if (!piper) {
    send({ type: "failed", id: message.id, message: "Giọng chưa được nạp." });
    return;
  }
  try {
    if (message.stream) {
      const mine = ++run;
      for (const sentence of listenPieces(message.text)) {
        if (mine !== run) return;
        const bits =
          message.lang === "vi"
            ? (await chunkText(await processTextForTTS(sentence))).filter((part) => part.trim() !== "")
            : [sentence];
        for (const bit of bits) {
          if (mine !== run) return;
          const pcm = await piper.piece(bit);
          if (mine !== run) return;
          send({ type: "chunk", id: message.id, rate: pcm.rate, samples: pcm.samples }, [pcm.samples.buffer]);
        }
      }
      if (mine !== run) return;
      send({ type: "done", id: message.id });
      return;
    }
    const pcm = message.whole
      ? await piper.speakPassage(message.text, message.lang)
      : await piper.speak(message.text, message.lang);
    send({ type: "pcm", id: message.id, rate: pcm.rate, samples: pcm.samples }, [pcm.samples.buffer]);
  } catch (caught) {
    send({ type: "failed", id: message.id, message: messageOf(caught) });
  }
}
