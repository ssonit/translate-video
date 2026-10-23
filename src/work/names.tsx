import { useId, useState } from "react";
import { mergeNameRows, parseNamePaste, pasteSummary } from "../namePaste";
import type { NameFix } from "./types";

export function NameForm({
  title,
  intro,
  note,
  error,
  rows,
  disabled,
  onChange,
  primary,
  secondary,
  onPrimary,
  onSecondary,
  onStop,
}: {
  title: string;
  intro: string;
  note: string;
  error: string;
  rows: NameFix[];
  disabled: boolean;
  onChange: (rows: NameFix[]) => void;
  primary: string;
  secondary: string;
  onPrimary: () => void;
  onSecondary: () => void;
  onStop?: () => void;
}) {
  const pasteId = useId();
  const [paste, setPaste] = useState("");
  const [pasteNote, setPasteNote] = useState("");
  const filled = rows.filter((row) => row.from.trim() || row.to.trim()).length;

  function applyPaste() {
    const parsed = parseNamePaste(paste);
    setPasteNote(pasteSummary(parsed));
    if (parsed.pairs.length === 0) return;
    onChange(mergeNameRows(rows, parsed.pairs));
    setPaste("");
  }

  return (
    <>
      <div className="pane-scroll names">
        <header className="names-intro">
          <h2>
            {title}
            {filled > 0 ? <span className="names-count">{filled}</span> : null}
          </h2>
          <p className="settings-note">{intro}</p>
          {note ? <p className="settings-note">{note}</p> : null}
          {error ? <p className="error">{error}</p> : null}
        </header>
        <div className="names-head">
          <span>Nghe được</span>
          <span />
          <span>Tên đúng</span>
          <span />
        </div>
        {rows.map((row, index) => (
          <div className="name-row" key={index}>
            <input
              aria-label="Nghe được"
              value={row.from}
              disabled={disabled}
              onChange={(event) =>
                onChange(rows.map((item, itemIndex) => (itemIndex === index ? { ...item, from: event.target.value } : item)))
              }
            />
            <span className="name-arrow" aria-hidden="true">
              →
            </span>
            <input
              aria-label="Tên đúng"
              value={row.to}
              disabled={disabled}
              onChange={(event) =>
                onChange(rows.map((item, itemIndex) => (itemIndex === index ? { ...item, to: event.target.value } : item)))
              }
            />
            <button
              type="button"
              className="name-x"
              aria-label={`Xóa ${row.from || "dòng này"}`}
              disabled={disabled}
              onClick={() => onChange(rows.filter((_, itemIndex) => itemIndex !== index))}
            >
              ×
            </button>
          </div>
        ))}
        <button
          type="button"
          className="names-add"
          disabled={disabled}
          onClick={() => onChange([...rows, { from: "", to: "" }])}
        >
          Thêm tên
        </button>
      </div>
      <div className="name-dock">
        <div className="name-paste">
          <span id={pasteId}>Dán cả danh sách</span>
          <div className="name-paste-row">
            <textarea
              aria-labelledby={pasteId}
              rows={2}
              value={paste}
              disabled={disabled}
              placeholder={"Lina = Lynae\nLy Na = Lynae"}
              onChange={(event) => setPaste(event.target.value)}
            />
            <button type="button" className="link" disabled={disabled || !paste.trim()} onClick={applyPaste}>
              Điền vào bảng
            </button>
          </div>
        </div>
        {pasteNote ? <p className="settings-note">{pasteNote}</p> : null}
      </div>
      <footer className="pane-foot">
        <span className="foot-actions">
          {onStop ? (
            <button type="button" className="link" onClick={onStop}>
              Dừng
            </button>
          ) : null}
          <button type="button" className="link" onClick={onSecondary} disabled={disabled}>
            {secondary}
          </button>
          <button type="button" className="check" onClick={onPrimary} disabled={disabled}>
            {primary}
          </button>
        </span>
      </footer>
    </>
  );
}
