import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import { errorText, fileName, keptLabel, languageName, sameFile } from "./format";
import type { KeptVersion, VersionDetails } from "./types";
import "./VersionHistory.css";

export function VersionHistory({ result, shown, items, blocked, onPlay, onApply, onDelete, inline = false }: {
  inline?: boolean; result: string; shown: string; items: KeptVersion[]; blocked: boolean;
  onPlay: (path: string) => void; onApply: (path: string) => void; onDelete: (item: KeptVersion) => void;
}) {
  const [open, setOpen] = useState(false);
  const [path, setPath] = useState(shown);
  const [info, setInfo] = useState<VersionDetails | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const epoch = useRef(0);
  const panel = useRef<HTMLDivElement>(null);
  useEffect(() => { setOpen(false); setInfo(null); epoch.current += 1; }, [result]);
  useEffect(() => {
    if (!open && !inline) return;
    let live = true;
    const ticket = ++epoch.current;
    setInfo(null); setLoading(true); setError("");
    invoke<VersionDetails>("version_details", { result, path }).then(detail => {
      if (live && ticket === epoch.current) setInfo(detail);
    }).catch(e => { if (live && ticket === epoch.current) setError(errorText(e)); })
      .finally(() => { if (live && ticket === epoch.current) setLoading(false); });
    return () => { live = false; };
  }, [open, inline, result, path]);
  useEffect(() => {
    if (!open || inline) return;
    const escape = (e: KeyboardEvent) => { if (e.key === "Escape") setOpen(false); };
    const outside = (e: PointerEvent) => { if (e.target instanceof Node && !panel.current?.contains(e.target)) setOpen(false); };
    document.addEventListener("keydown", escape); document.addEventListener("pointerdown", outside);
    return () => { document.removeEventListener("keydown", escape); document.removeEventListener("pointerdown", outside); };
  }, [open, inline]);
  useEffect(() => { if (inline) setPath(shown); }, [shown, inline]);
  const cfg = info?.config;
  return <div className={inline ? "version-history is-inline" : "version-history"} ref={panel}>
    {!inline ? <button type="button" className="link" aria-expanded={open} onClick={() => { if (!open) setPath(shown); setOpen(!open); }}>Lịch sử &amp; cấu hình ({items.length})</button> : null}
    {open || inline ? <section className="version-history-panel" aria-label="Lịch sử bản xuất">
      <header><strong>Lịch sử bản xuất</strong>{!inline ? <button type="button" className="link" onClick={() => setOpen(false)}>Đóng lịch sử</button> : null}</header>
      {blocked ? <p className="history-note">Hoàn tất việc đang chạy và xuất hoặc bỏ nháp đang sửa trước khi đổi phiên bản hay nạp cấu hình.</p> : null}
      <button type="button" className="link" onClick={() => setPath(shown)}>Cấu hình bản đang xem</button>
      <ol className="version-history-list">
        {items.map(item => <li key={item.path} className={sameFile(shown,item.path) ? "is-selected" : ""}>
          <div><strong>{item.audio ? "Audio" : "Video"} · {keptLabel(item)}</strong><small>{item.summary || "Bản cũ · không rõ thay đổi"}</small></div>
          <span>
            <button type="button" className="link" disabled={blocked} onClick={() => { setOpen(false); onPlay(item.path); }}>Xem</button>
            <button type="button" className="link" onClick={() => setPath(item.path)}>Cấu hình</button>
            <button type="button" className="link" disabled={blocked} onClick={() => onDelete(item)}>Xóa</button>
          </span>
        </li>)}
      </ol>
      {items.length === 0 ? <p className="history-note">Chưa có bản cũ khác. Các lần xuất thành công tiếp theo sẽ được giữ tại đây.</p> : null}
      <div className="version-history-details" aria-busy={loading}>
        <strong>{fileName(path)}</strong>
        {loading ? <p role="status">Đang đọc thông tin bản xuất…</p> : null}
        {error ? <p className="error" role="status">{error}</p> : null}
        {info ? <>
          <p>{info.reasons.join(" · ")}</p>
          {info.createdMs > 0 ? <small>Lưu lúc {new Date(info.createdMs).toLocaleString("vi-VN")}</small> : null}
          {cfg ? <dl>
            <dt>Kết quả</dt><dd>{cfg.audioOnly ? "Audio" : cfg.textOnly ? "Video phụ đề" : cfg.burnCaptions ? "Video lồng tiếng có chữ" : "Video lồng tiếng"}</dd>
            <dt>Ngôn ngữ / giọng</dt><dd>{languageName(cfg.target)} · {cfg.textOnly ? "Không đọc giọng" : cfg.voice || "Chưa ghi nhận"}</dd>
            <dt>Khung hình</dt><dd>{cfg.render.aspect === "original" ? "Khung gốc" : cfg.render.aspect} · {cfg.render.framing === "crop" ? "Cắt đầy khung" : "Nền mờ"}</dd>
            <dt>Kiểu chữ</dt><dd>{cfg.render.font} · {cfg.render.size}% · <span className="history-color" style={{ background: cfg.render.color }} /> {cfg.render.color}</dd>
            <dt>Âm lượng</dt><dd>Gốc {cfg.render.audio.original}% · Lồng {cfg.render.audio.voice}%{cfg.render.audio.duck ? " · Tự hạ tiếng gốc" : ""}</dd>
            <dt>Logo / che chữ</dt><dd>{cfg.render.logo.enabled ? "Có logo" : "Không logo"} · {cfg.render.mask.enabled ? "Có vùng che" : "Không vùng che"}</dd>
            <dt>Dịch</dt><dd>{cfg.skipTranslate ? "Không dịch" : cfg.translation ? `${cfg.translation.provider} · ${cfg.translation.model}` : "Chưa ghi nhận dịch vụ"}{cfg.useTerms ? " · Có thuật ngữ" : ""}</dd>
            {info.lines !== null ? <><dt>Số câu</dt><dd>{info.lines}</dd></> : null}
          </dl> : <p className="history-note">Bản này chưa có cấu hình đã xác nhận để nạp.</p>}
          <div className="history-files">{info.files.map(file => <button key={file.path} type="button" className="link" disabled={blocked} onClick={() => { setOpen(false); onPlay(file.path); }}>
            Xem {file.kind === "audio" ? "audio" : file.kind === "captioned" ? "video có chữ" : "video không chữ"}
          </button>)}</div>
          <p className="history-note">{info.note}</p>
          <button type="button" className="check" disabled={blocked || !cfg || loading} onClick={() => { setOpen(false); onApply(path); }}>Nạp cấu hình để sửa tiếp</button>
        </> : null}
      </div>
    </section> : null}
  </div>;
}
