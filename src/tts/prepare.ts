import { chunkText, processTextForTTS } from "./vendor/text-cleaner.js";

// The Vietnamese cleaner (numbers, dates, acronyms, foreign words into Vietnamese sounds) would
// ruin other languages, so they only get emoji and stray symbols removed and a split into sentences.
function plainText(raw: string): string {
  return raw
    .replace(/[\u{1F000}-\u{1FAFF}\u{2600}-\u{27BF}\u{FE0F}\u{200D}]/gu, "")
    .replace(/[\\()"“”]/g, "")
    .replace(/\s+/g, " ")
    .trim();
}

export function plainPassage(raw: string): string {
  return plainText(raw);
}

export function plainChunks(raw: string): string[] {
  const text = plainText(raw);
  if (!text) return [];
  return text
    .split(/(?<=[.!?。！？])\s*/u)
    .map((part) => part.trim())
    .filter((part) => part !== "");
}

// Cheap split for listening. The first sentence can be spoken before the rest is normalized.
export function listenPieces(raw: string): string[] {
  const pieces: string[] = [];
  for (const line of raw.split(/\n+/)) {
    pieces.push(...plainChunks(line));
  }
  return pieces;
}

export async function prepareLine(text: string, lang: string): Promise<string[]> {
  if (lang === "vi") {
    return (await chunkText(await processTextForTTS(text))).filter((chunk) => chunk.trim() !== "");
  }
  return plainChunks(text);
}
