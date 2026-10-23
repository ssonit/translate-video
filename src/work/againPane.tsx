import { languageName, LANGUAGES } from "./format";
import { NameForm } from "./names";
import type { NameFix } from "./types";

export function AgainPane({
  summary,
  lang,
  voice,
  voices,
  retranscribe,
  textOnly,
  note,
  rows,
  running,
  onLang,
  onVoice,
  onRetranscribe,
  onTextOnly,
  onRows,
  onRun,
  onClose,
}: {
  summary: string;
  lang: string;
  voice: string;
  voices: string[];
  retranscribe: boolean;
  textOnly: boolean;
  note: string;
  rows: NameFix[];
  running: boolean;
  onLang: (code: string) => void;
  onVoice: (name: string) => void;
  onRetranscribe: (on: boolean) => void;
  onTextOnly: (on: boolean) => void;
  onRows: (rows: NameFix[]) => void;
  onRun: () => void;
  onClose: () => void;
}) {
  return (
    <div className="pane-fill">
      <div className="again-bar">
        <div className="again-title">
          <h2>Làm lại</h2>
          <p className={summary === "Chưa đổi ngôn ngữ, giọng hay tên." ? "again-summary" : "again-summary is-on"}>{summary}</p>
        </div>
        <div className="again-picks">
          <label>
            Ngôn ngữ
            <select className="pill-select" value={lang} disabled={running} onChange={(event) => onLang(event.target.value)}>
              {LANGUAGES.map((item) => (
                <option key={item.code} value={item.code}>
                  {item.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            Giọng đọc
            {voices.length > 0 ? (
              <select
                className="pill-select"
                value={voice}
                aria-label="Giọng khi làm lại"
                disabled={running}
                onChange={(event) => onVoice(event.target.value)}
              >
                {voices.map((name) => (
                  <option key={name} value={name}>
                    {name}
                  </option>
                ))}
              </select>
            ) : (
              <p className="again-missing">Chưa có giọng cho {languageName(lang)}. Bỏ cặp file vào voices\{lang}.</p>
            )}
          </label>
          <label className="again-check">
            <input type="checkbox" checked={retranscribe} disabled={running} onChange={(event) => onRetranscribe(event.target.checked)} />
            Chép lại lời
          </label>
          <label className="again-check" title="Giữ tiếng gốc. Đốt chữ dịch lên hình. Không đọc giọng mới.">
            <input type="checkbox" checked={textOnly} disabled={running} onChange={(event) => onTextOnly(event.target.checked)} />
            Chỉ hiện chữ
          </label>
        </div>
        {note ? <p className="error">{note}</p> : null}
      </div>
      <NameForm
        title="Tên"
        intro="Tên này dùng khi dịch lại. Câu không đổi thì giữ bản dịch cũ."
        note=""
        error=""
        rows={rows}
        disabled={running}
        onChange={onRows}
        primary="Làm lại"
        secondary="Huỷ"
        onPrimary={() => void onRun()}
        onSecondary={onClose}
      />
    </div>
  );
}
