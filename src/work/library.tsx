import { useState } from "react";
import { jobFolder, languageName, openJobFolder, plainVideo, sameFile, when } from "./format";
import { filterLibrary, type LibraryFilter } from "./libraryFilter";
import { LibraryPreview } from "./LibraryPreview";
import type { FinishedDub, View } from "./types";

export function Library({
  view,
  queueCount,
  query,
  onQuery,
  onNew,
  showLiveCard,
  liveTitle,
  liveDetail,
  liveFraction,
  returnPreview,
  previewLabel,
  onProgress,
  onLive,
  dubs,
  note,
  removing,
  result,
  draft,
  captioning,
  isLive,
  isPreview,
  onOpenDone,
  onOpenDraft,
  onRemove,
}: {
  view: View;
  queueCount: number;
  query: string;
  onQuery: (value: string) => void;
  onNew: () => void;
  showLiveCard: boolean;
  liveTitle: string;
  liveDetail: string;
  liveFraction: number | null;
  returnPreview: boolean;
  previewLabel: string;
  onProgress: () => void;
  onLive: () => void;
  dubs: FinishedDub[];
  note: string;
  removing: string;
  result: string;
  draft: FinishedDub | null;
  captioning: boolean;
  isLive: (item: FinishedDub) => boolean;
  isPreview: (item: FinishedDub) => boolean;
  onOpenDone: (item: FinishedDub) => void;
  onOpenDraft: (item: FinishedDub) => void;
  onRemove: (item: FinishedDub) => void;
}) {
  const [filter, setFilter] = useState<LibraryFilter>("all");
  const visible = filterLibrary(dubs, query, filter);
  const liveOn = view === "progress" || (returnPreview && view === "live");

  return (
    <aside className="library">
      <div className="studio-tabs library-filters" aria-label="Lọc thư viện">
        {([["all", "Tất cả"], ["draft", "Đang dở"], ["done", "Đã xong"]] as const).map(([id, label]) => <button key={id} type="button" aria-pressed={filter === id} className={filter === id ? "is-active" : ""} onClick={() => setFilter(id)}>{label} · {dubs.filter(item => id === "all" || item.done === (id === "done")).length}</button>)}
      </div>
      <label className="library-find">
        <span className="library-find-label">Tìm video</span>
        <svg className="library-find-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
          <circle cx="10.5" cy="10.5" r="6.5" />
          <path d="m16 16 4.5 4.5" />
        </svg>
        <input type="search" value={query} placeholder="Tìm theo tên" onChange={(event) => onQuery(event.target.value)} />
      </label>
      <div className={view === "new" ? "lib-item is-on library-pin" : "lib-item library-pin"}>
        <button type="button" className="row" onClick={onNew}>
          <span>
            <strong>Video mới</strong>
            <em>{queueCount > 0 ? `${queueCount} video đang chờ` : "Chọn nguồn và chạy"}</em>
          </span>
        </button>
      </div>
      <div className="library-scroll">
        {showLiveCard ? (
          <div className={liveOn ? "lib-item is-on" : "lib-item"}>
            <button type="button" className="row" onClick={onProgress}>
              <span>
                <strong title={liveTitle}>{liveTitle}</strong>
                <em title={liveDetail}>
                  {liveDetail}
                  {liveFraction !== null ? ` · ${Math.round(liveFraction * 100)}%` : ""}
                </em>
                {liveFraction !== null ? (
                  <span className="mini" aria-hidden="true">
                    <i style={{ width: `${liveFraction * 100}%` }} />
                  </span>
                ) : null}
              </span>
            </button>
            {returnPreview ? (
              <button type="button" className="row-back" onClick={onLive}>
                {previewLabel}
              </button>
            ) : null}
          </div>
        ) : null}
        {visible.map((item) => {
          const live = isLive(item);
          const back = isPreview(item);
          const selected =
            (item.done && view === "watch" && sameFile(item.path, plainVideo(result))) ||
            (!item.done && view === "resume" && draft?.path === item.path) ||
            (live && view === "progress") ||
            (back && (view === "live" || view === "progress"));
          const writing = live || (captioning && sameFile(jobFolder(item), openJobFolder(result)));
          const openItem = () => {
            if (!item.done) {
              if (live || back) {
                onProgress();
                return;
              }
              onOpenDraft(item);
              return;
            }
            onOpenDone(item);
          };
          return (
            <div key={item.path} className={selected ? "lib-item is-on" : "lib-item"}>
              <button type="button" className="row" onClick={openItem}>
                <LibraryPreview path={item.done ? item.path : item.video} audio={item.audio} />
                <span>
                  <strong title={item.title}>{item.title}</strong>
                  <em title={live ? liveDetail : undefined}>
                    {item.done
                      ? [item.audio ? "chỉ audio" : "", item.language ? languageName(item.language) : "", item.captioned ? "có chữ" : "", when(item.modifiedMs)]
                          .filter(Boolean)
                          .join(" · ")
                      : live
                        ? `${liveDetail}${liveFraction !== null ? ` · ${Math.round(liveFraction * 100)}%` : ""}`
                        : `Đang dở · ${when(item.modifiedMs)}`}
                  </em>
                  <small className="library-source" title={item.url || item.video}>{item.url || item.video || "Nguồn trong folder việc"}</small>
                  {live && liveFraction !== null ? (
                    <span className="mini" aria-hidden="true">
                      <i style={{ width: `${liveFraction * 100}%` }} />
                    </span>
                  ) : null}
                </span>
              </button>
              {back ? (
                <button type="button" className="row-back" onClick={onLive}>
                  {previewLabel}
                </button>
              ) : null}
              {writing ? null : (
                <button
                  type="button"
                  className="row-x"
                  aria-label={`Xóa ${item.title}`}
                  disabled={removing === item.path}
                  onClick={() => void onRemove(item)}
                >
                  {removing === item.path ? "…" : "Xóa"}
                </button>
              )}
            </div>
          );
        })}
        {dubs.length === 0 && !showLiveCard ? <p className="library-empty">Folder này chưa có video.</p> : null}
        {dubs.length > 0 && visible.length === 0 ? <p className="library-empty">Không thấy video khớp.</p> : null}
        {note ? <p className="error library-note">{note}</p> : null}
      </div>
    </aside>
  );
}
