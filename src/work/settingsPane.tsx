export function SettingsPane({
  folder,
  running,
  useTerms,
  skipTranslate,
  note,
  onFolder,
  onTerms,
}: {
  folder: string;
  running: boolean;
  useTerms: boolean;
  skipTranslate: boolean;
  note: string;
  onFolder: () => void;
  onTerms: (on: boolean) => void;
}) {
  return (
    <div className="pane-fill">
      <div className="pane-scroll composer">
        <article className="path">
          <div>
            <strong>Folder kết quả</strong>
            <p title={folder}>{folder || "Chưa chọn"}</p>
          </div>
          <button type="button" className="check" onClick={() => void onFolder()} disabled={running}>
            Đổi
          </button>
        </article>
        <label className="redo settings-terms">
          <input
            type="checkbox"
            checked={useTerms}
            disabled={running || skipTranslate}
            onChange={(event) => void onTerms(event.target.checked)}
          />
          Lấy thuật ngữ trước khi dịch
        </label>
        <p className="settings-note">
          {skipTranslate
            ? "Đang chỉ đổi giọng, nên không lấy thuật ngữ."
            : "Bật thì AI rút tên riêng và thuật ngữ để các câu dịch thống nhất. Tắt thì dịch thẳng."}
        </p>
        {note ? <p className="error">{note}</p> : null}
      </div>
    </div>
  );
}
