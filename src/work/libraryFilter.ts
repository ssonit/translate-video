import { fold, languageName } from "./format.ts";
import type { FinishedDub } from "./types.ts";

export type LibraryFilter = "all" | "draft" | "done";

export function filterLibrary(items: FinishedDub[], query: string, filter: LibraryFilter) {
  const needle = fold(query.trim());
  return items.filter(item => (filter === "all" || item.done === (filter === "done"))
    && (!needle || fold(`${item.title} ${languageName(item.language)} ${item.language} ${item.video} ${item.url ?? ""}`).includes(needle)));
}
