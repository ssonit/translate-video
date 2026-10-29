import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { LockedNote } from "../SetupChip";
import { draftProgress, isAudio, jobFolder, languageName, when } from "./format";
import { RunOptions } from "./options";
import type { DraftBrief, FinishedDub } from "./types";

// Hình video gốc và vài câu đầu của việc dở, đọc từ folder việc. Không đọc được thì thẻ chỉ có tên như trước.
function useDraftPreview(draft: FinishedDub) {
  const [loaded, setLoaded] = useState<{ path: string; brief: DraftBrief } | null>(null);
  const [opened, setOpened] = useState<{ video: string; src: string } | null>(null);

  useEffect(() => {
    let live = true;
    invoke<DraftBrief>("draft_brief", { path: draft.path })
      .then((brief) => live && setLoaded({ path: draft.path, brief }))
      .catch(() => live && setLoaded(null));
    return () => {
      live = false;
    };
  }, [draft.path, draft.modifiedMs]);

  useEffect(() => {
    let live = true;
    if (draft.video) {
      invoke("allow_result", { path: draft.video })
        .then(() => live && setOpened({ video: draft.video, src: convertFileSrc(draft.video) }))
        .catch(() => live && setOpened(null));
    }
    return () => {
      live = false;
    };
  }, [draft.video]);

  return {
    brief: loaded?.path === draft.path ? loaded.brief : null,
    src: opened?.video === draft.video ? opened.src : "",
  };
}

export function ResumePane({
  draft,
  audioOnly,
  textOnly,
  burnCaptions,
  captioning,
  running,
  onAudioOnly,
  onTextOnly,
  onBurn,
  onNames,
  onLeave,
  onResume,
  onEdit,
  lock,
  onTools,
}: {
  draft: FinishedDub;
  audioOnly: boolean;
  textOnly: boolean;
  burnCaptions: boolean;
  captioning: boolean;
  running: boolean;
  onAudioOnly: (on: boolean) => void;
  onTextOnly: (on: boolean) => void;
  onBurn: (on: boolean) => void;
  onNames: (root: string) => void;
  onLeave: () => void;
  onResume: (item: FinishedDub) => void;
  onEdit: () => void;
  // Dòng báo thiếu mục bắt buộc. Trống thì không khóa.
  lock: string;
  onTools: () => void;
}) {
  const { brief, src } = useDraftPreview(draft);
  const [badSrc, setBadSrc] = useState("");
  const meta = [draft.language ? languageName(draft.language) : "", draft.modifiedMs ? `lần cuối ${when(draft.modifiedMs)}` : ""]
    .filter(Boolean)
    .join(" · ");
  return (
    <div className="pane-fill">
      <div className="pane-scroll composer hug">
        <article className="path draft">
          {src && src !== badSrc ? (
            isAudio(draft.video) ? (
              <audio className="draft-media" src={src} aria-label="Video gốc" controls preload="metadata" onError={() => setBadSrc(src)} />
            ) : (
              <video className="draft-media" src={`${src}#t=0.1`} aria-label="Video gốc" controls preload="metadata" onError={() => setBadSrc(src)} />
            )
          ) : null}
          <div>
            <strong>{draft.title}</strong>
            {brief ? <p className="draft-status">{draftProgress(brief)}</p> : null}
            {meta ? <p className="draft-meta">{meta}</p> : null}
            {draft.url ? <p className="draft-meta draft-link">{draft.url}</p> : null}
            {brief && brief.excerpt.length > 0 ? (
              <blockquote className="draft-quote">
                <span>{brief.excerptTranslated ? "Mở đầu video" : "Mở đầu video (lời gốc)"}</span>
                <p>{brief.excerpt.join(" ")}</p>
              </blockquote>
            ) : null}
            <p className="draft-note">Việc này đang dở. Phần đã làm vẫn giữ. Chạy tiếp không tải lại và không chép lời lại.</p>
          </div>
        </article>
        <RunOptions
          audioOnly={audioOnly}
          textOnly={textOnly}
          burnCaptions={burnCaptions}
          disabled={captioning}
          onAudioOnly={onAudioOnly}
          onTextOnly={onTextOnly}
          onBurn={onBurn}
        />
      </div>
      <footer className="pane-foot">
        <LockedNote note={lock} onTools={onTools} />
        <span className="foot-actions">
          <button type="button" className="link" onClick={onEdit} disabled={running || captioning}>Mở nháp & sửa</button>
          <button type="button" className="link" onClick={() => void onNames(jobFolder(draft))} disabled={running}>
            Sửa tên
          </button>
          <button type="button" className="link" onClick={onLeave}>
            Để đó
          </button>
          <button
            type="button"
            className={lock ? "check is-idle" : "check"}
            onClick={() => void onResume(draft)}
            disabled={lock !== "" || captioning || draft.video === ""}
          >
            {running ? "Thêm vào hàng" : "Chạy tiếp"}
          </button>
        </span>
      </footer>
      {!draft.video ? <p className="error">Không thấy video gốc để chạy tiếp.</p> : null}
    </div>
  );
}
