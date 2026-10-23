import { jobFolder } from "./format";
import { RunOptions } from "./options";
import type { FinishedDub } from "./types";

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
}) {
  return (
    <div className="pane-fill">
      <div className="pane-scroll composer hug">
        <article className="path">
          <div>
            <strong>{draft.title}</strong>
            <p>Việc này đang dở. Phần đã làm vẫn giữ. Chạy tiếp không tải lại và không chép lời lại.</p>
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
        <span className="foot-actions">
          <button type="button" className="link" onClick={() => void onNames(jobFolder(draft))} disabled={running}>
            Sửa tên
          </button>
          <button type="button" className="link" onClick={onLeave}>
            Để đó
          </button>
          <button type="button" className="check" onClick={() => void onResume(draft)} disabled={captioning || draft.video === ""}>
            {running ? "Thêm vào hàng" : "Chạy tiếp"}
          </button>
        </span>
      </footer>
      {!draft.video ? <p className="error">Không thấy video gốc để chạy tiếp.</p> : null}
    </div>
  );
}
