// Phonemizer returns one string per clause. Put the clause separators of the
// original text back so the model hears the pauses.
export function mergePhonemes(text: string, parts: string[]): string {
  const separators = Array.from(text.matchAll(/[,;:.!?]/g), (match) => match[0]);
  let result = "";
  let used = 0;
  for (const raw of parts) {
    const part = String(raw ?? "").trim();
    if (!part) continue;
    if (result) {
      result += `${separators[used] ?? ","} `;
      used += 1;
    }
    result += part;
  }
  return result;
}

// The model stops on the last sound if nothing follows it. A pause mark lets that word finish.
export function endMark(phonemes: string): string {
  const trimmed = phonemes.trim();
  if (!trimmed || /[.!?,;:]$/.test(trimmed)) return trimmed;
  return `${trimmed}.`;
}
