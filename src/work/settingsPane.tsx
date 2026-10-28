export function SettingsPane({
  folder,
  running,
  useTerms,
  skipTranslate,
  note,
  onFolder,
  onTerms,
  onBack,
  onTools,
}: {
  folder: string;
  running: boolean;
  useTerms: boolean;
  skipTranslate: boolean;
  note: string;
  onFolder: () => void;
  onTerms: (on: boolean) => void;
  onBack: () => void;
  onTools: () => void;
}) {
  return (
    <div className="pane-fill preferences-pane">
      <div className="pane-scroll preferences-scroll">
        <header className="preferences-heading">
          <p className="kicker">Không gian làm việc</p>
          <h2>Cài đặt</h2>
          <p>Một vài lựa chọn để những lần lồng tiếng sau thuận tiện hơn.</p>
        </header>

        {running ? <p className="preferences-notice" role="status">Đang xử lý video. Bạn có thể đổi các lựa chọn này khi việc hiện tại hoàn tất.</p> : null}

        <section className="preference-card" aria-labelledby="storage-heading">
          <div className="preference-card-heading">
            <span className="preference-symbol" aria-hidden="true">01</span>
            <div>
              <h3 id="storage-heading">Nơi lưu kết quả</h3>
              <p>Video, audio và các file của từng việc được lưu tại đây.</p>
            </div>
          </div>
          <div className="preference-folder">
            <div><span>Folder kết quả</span><p title={folder}>{folder || "Chưa chọn folder"}</p></div>
            <button type="button" className="link" onClick={onFolder} disabled={running}>Đổi folder</button>
          </div>
          <p className="preference-hint">Mỗi video có một folder riêng để bạn có thể chạy tiếp khi cần.</p>
        </section>

        <section className="preference-card" aria-labelledby="translation-heading">
          <div className="preference-card-heading">
            <span className="preference-symbol" aria-hidden="true">02</span>
            <div>
              <h3 id="translation-heading">Bản dịch nhất quán</h3>
              <p>Hữu ích với video có nhiều tên riêng hoặc từ chuyên ngành.</p>
            </div>
          </div>
          <label className="preference-toggle">
            <span>
              <strong>Lấy thuật ngữ trước khi dịch</strong>
              <span id="terms-description">{skipTranslate
                ? "Đang chỉ đổi giọng, nên không lấy thuật ngữ."
                : "AI rút tên riêng và thuật ngữ để dùng thống nhất giữa các câu. Khi tắt, app dịch thẳng."}</span>
            </span>
            <input type="checkbox" role="switch" aria-describedby="terms-description" checked={useTerms} disabled={running || skipTranslate} onChange={(event) => onTerms(event.target.checked)} />
          </label>
        </section>

        <button type="button" className="preference-tools" onClick={onTools}>
          <span><strong>Đồ nghề & giọng đọc</strong><span>Kiểm tra công cụ, chọn giọng Việt hoặc thiết lập dịch vụ dịch.</span></span>
          <span aria-hidden="true">↗</span>
        </button>
        <p className="preference-save-note">Các thay đổi được lưu tự động trên máy.</p>
        {note ? <p className="error" role="alert">{note}</p> : null}
      </div>
      <footer className="preferences-footer">
        <p>Video và audio được xử lý trên máy.</p>
        <button type="button" className="check" onClick={onBack}>Về lồng tiếng <span aria-hidden="true">→</span></button>
      </footer>
    </div>
  );
}
