import { useState } from "react";
import type { ChannelPreset } from "./renderSettings";
import { languageName } from "./format";

export function PresetLibrary({ presets, disabled, note, onApply, onDelete, onRename, onSave, onConfigure }: {
  presets: ChannelPreset[]; disabled: boolean; note: string;
  onApply: (preset: ChannelPreset) => void; onDelete: (id: string) => void;
  onRename: (id: string, name: string) => void; onSave: (name: string) => void; onConfigure: () => void;
}) {
  const [name, setName] = useState("");
  const [renaming, setRenaming] = useState("");
  const [rename, setRename] = useState("");
  return <div className="studio-page pane-scroll">
    <header className="studio-page-heading"><p className="kicker">Cấu hình dùng lại</p><h2>Bộ cài đặt</h2><p>Lưu phong cách cho từng kênh. Áp dụng vào video mới; các việc đã xếp hàng giữ lựa chọn riêng.</p></header>
    <section className="studio-card"><h3>Lưu lựa chọn hiện tại</h3><p>Gồm khung hình, chữ, âm thanh, ngôn ngữ, giọng và loại kết quả. Không lưu API key.</p>
      <form className="studio-inline-form" onSubmit={e => { e.preventDefault(); if (name.trim()) { onSave(name.trim()); setName(""); } }}>
        <input aria-label="Tên bộ cài đặt mới" placeholder="Tên kênh hoặc phong cách" maxLength={60} value={name} disabled={disabled} onChange={e => setName(e.target.value)} />
        <button className="check" disabled={disabled || !name.trim()}>Lưu bộ mới</button>
        <button type="button" className="link" onClick={onConfigure}>Chỉnh cấu hình</button>
      </form>
    </section>
    <div className="preset-grid">{presets.map(preset => <article className="studio-card preset-card" key={preset.id}>
      <span className="preset-aspect">{preset.audioOnly ? "♪" : preset.render.aspect === "original" ? "Gốc" : preset.render.aspect}</span>
      <h3>{preset.name}</h3><p>{languageName(preset.target)} · {preset.audioOnly ? "Audio" : preset.textOnly ? "Video phụ đề" : "Video lồng tiếng"}</p>
      <p>{preset.textOnly ? "Giữ tiếng gốc" : preset.voice || "Chưa chọn giọng"}{preset.burnCaptions && !preset.audioOnly ? " · Có phụ đề" : ""}</p>
      {renaming === preset.id ? <form className="studio-inline-form" onSubmit={e => { e.preventDefault(); if (rename.trim()) { onRename(preset.id, rename.trim()); setRenaming(""); } }}>
        <input autoFocus aria-label="Đổi tên bộ cài đặt" maxLength={60} value={rename} onChange={e => setRename(e.target.value)} />
        <button className="check" disabled={disabled || !rename.trim()}>Lưu tên</button><button type="button" className="link" onClick={() => setRenaming("")}>Hủy</button>
      </form> : <div className="studio-card-actions">
        <button type="button" className="check" disabled={disabled} onClick={() => onApply(preset)}>Dùng bộ này</button>
        <button type="button" className="link" disabled={disabled} onClick={() => { setRenaming(preset.id); setRename(preset.name); }}>Đổi tên</button>
        <button type="button" className="link" disabled={disabled} onClick={() => onDelete(preset.id)}>Xóa</button>
      </div>}
    </article>)}</div>
    {presets.length === 0 ? <div className="studio-empty"><strong>Chưa có bộ cài đặt</strong><p>Chỉnh cấu hình cho kênh của bạn rồi lưu để dùng ở những video sau.</p></div> : null}
    {note ? <p role="status" className="error">{note}</p> : null}
  </div>;
}
