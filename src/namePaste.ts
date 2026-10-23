export type NamePair = { from: string; to: string };

export type NamePaste = {
  pairs: NamePair[];
  skippedMeanings: number;
  skippedNames: number;
};

const PAIR_MARK = /^(.+?)\s*(?:=|→|->|=>)\s*(.+)$/;

function stripIndex(value: string): string {
  return value.replace(/^\d+[.)]\s*/, "").trim();
}

function splitPair(line: string): { from: string; to: string } | null {
  const cleaned = line.replace(/^\|+|\|+$/g, "").trim();
  if (!cleaned || /^[-:|.\s]+$/.test(cleaned)) return null;
  const tabbed = cleaned
    .split("\t")
    .map((part) => part.trim())
    .filter(Boolean);
  if (tabbed.length >= 2) {
    return { from: stripIndex(tabbed[0]), to: tabbed.slice(1).join(" ").trim() };
  }
  const marked = cleaned.match(PAIR_MARK);
  if (marked) return { from: stripIndex(marked[1].trim()), to: marked[2].trim() };
  const wide = cleaned.match(/^(.+?)\s{2,}(.+)$/);
  if (wide) return { from: stripIndex(wide[1].trim()), to: wide[2].trim() };
  return null;
}

function isHeader(from: string, to: string): boolean {
  return /^thuật ngữ/i.test(from) && /^(ý nghĩa|giải thích)/i.test(to);
}

// Cột nghĩa trong bài lore dài hơn một tên. Đưa vào bảng tên sẽ thay cả địa danh bằng câu giải thích.
function isMeaning(to: string): boolean {
  const words = to.trim().split(/\s+/).filter(Boolean);
  if (words.length >= 8) return true;
  return to.length >= 48 && /[,.]/.test(to);
}

function isBareName(line: string): boolean {
  const bare = stripIndex(line.replace(/^[-*•]\s*/, "")).trim();
  if (!bare || bare.length > 60 || /[.!?]/.test(bare)) return false;
  if (!/\p{L}/u.test(bare)) return false;
  if (bare.split(/\s+/).length > 6) return false;
  if (
    /^(thuật ngữ|ý nghĩa|wuthering|fandom|lưu ý|nếu bạn|theo tài liệu|đây là|các địa danh)/i.test(bare) ||
    /thuật ngữ|cơ chế chiến|gameplay|tổ chức và|cốt truyện|địa danh/i.test(bare)
  ) {
    return false;
  }
  return true;
}

export function parseNamePaste(text: string): NamePaste {
  const pairs: NamePair[] = [];
  const seen = new Set<string>();
  let skippedMeanings = 0;
  let skippedNames = 0;
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim();
    if (!line) continue;
    const pair = splitPair(line);
    if (!pair || !pair.from || !pair.to) {
      if (isBareName(line)) skippedNames += 1;
      continue;
    }
    if (isHeader(pair.from, pair.to)) continue;
    if (isMeaning(pair.to)) {
      skippedMeanings += 1;
      continue;
    }
    const key = pair.from.toLocaleLowerCase();
    if (seen.has(key) || key === pair.to.toLocaleLowerCase()) continue;
    seen.add(key);
    pairs.push({ from: pair.from, to: pair.to });
  }
  return { pairs, skippedMeanings, skippedNames };
}

export function mergeNameRows(rows: NamePair[], pairs: NamePair[]): NamePair[] {
  const next = rows.filter((row) => row.from.trim() || row.to.trim());
  const seen = new Set(next.map((row) => row.from.trim().toLocaleLowerCase()).filter(Boolean));
  for (const pair of pairs) {
    const key = pair.from.toLocaleLowerCase();
    if (seen.has(key)) continue;
    seen.add(key);
    next.push(pair);
  }
  return next.length ? next : [{ from: "", to: "" }];
}

export function pasteSummary(parsed: NamePaste): string {
  if (parsed.pairs.length === 0) {
    return "Chưa thêm dòng nào. Mỗi dòng một cặp: chữ nghe được, dấu =, tên đúng. Ví dụ: Lina = Lynae.";
  }
  const bits = [`Đã thêm ${parsed.pairs.length} tên.`];
  if (parsed.skippedMeanings) {
    bits.push(`${parsed.skippedMeanings} dòng giải nghĩa nằm ngoài bảng.`);
  }
  if (parsed.skippedNames) {
    bits.push(`${parsed.skippedNames} dòng chỉ có tên đúng, cần thêm chữ nghe được.`);
  }
  return bits.join(" ");
}
