// Suggestions are only for the review form, never committed automatically.
export function splitSuggestion(text: string, fraction: number): [string, string] {
  type Segmenter = { segment(text: string): Iterable<{ segment: string }> };
  const Constructor = (Intl as unknown as { Segmenter?: new (locale: string, options: { granularity: string }) => Segmenter }).Segmenter;
  if (!Constructor) return [text, ""];
  const clusters = [...new Constructor("vi", { granularity: "grapheme" }).segment(text)].map(part => part.segment);
  if (clusters.length < 2) return [text, ""];
  let at = Math.max(1, Math.min(clusters.length - 1, Math.round(clusters.length * fraction)));
  for (let distance = 0; distance < Math.min(12, clusters.length / 3); distance++) {
    const found = [at + distance, at - distance].find(i => i > 0 && i < clusters.length && /^\s+$/u.test(clusters[i]));
    if (found !== undefined) { at = found; break; }
  }
  return [clusters.slice(0, at).join("").trim(), clusters.slice(at).join("").trim()];
}

export function structureTextProblem(...texts: string[]) {
  return texts.some(text => !text.trim() || [...text].length > 4000) ? "Điền đủ lời gốc và lời đọc cho mỗi phần, tối đa 4.000 ký tự." : "";
}
