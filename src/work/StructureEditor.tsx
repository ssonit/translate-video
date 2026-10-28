import { useEffect, useRef, useState, type RefObject } from "react";
import { splitSuggestion, structureTextProblem } from "./structure";
import type { ScriptLine, StructureEdit } from "./types";

type Form = { kind: "split" | "merge" | "undo"; at: string; leftSource: string; leftText: string; rightSource: string; rightText: string; reviewed: boolean };

export function StructureEditor({ lines, selected, revision, disabled, pending, videoRef, onApply }: {
  lines: ScriptLine[]; selected: number; revision: string; disabled: boolean; pending: boolean;
  videoRef: RefObject<HTMLMediaElement | null>; onApply: (action: StructureEdit) => void;
}) {
  const [form, setForm] = useState<Form | null>(null);
  const [note, setNote] = useState("");
  const reviewRef = useRef<HTMLDivElement>(null);
  const current = lines[selected];
  const next = lines[selected + 1];
  const blocked = disabled || pending || !current?.id;
  useEffect(() => { setForm(null); setNote(""); }, [selected, lines]);
  useEffect(() => {
    const review = reviewRef.current;
    const panel = review?.closest<HTMLElement>(".timing-panel");
    if (review && panel) panel.scrollTop += review.getBoundingClientRect().top - panel.getBoundingClientRect().top - 8;
  }, [form?.kind]);

  function open(kind: Form["kind"]) {
    if (blocked) return;
    setNote("");
    const at = videoRef.current?.currentTime ?? NaN;
    videoRef.current?.pause();
    if (kind === "split" && (!Number.isFinite(at) || at - current.start < .08 || current.end - at < .08)) {
      setNote("Phát hoặc bấm vào timeline để đặt vị trí trong câu đang chọn; mỗi phần cần ít nhất 0,08 giây."); return;
    }
    if (kind === "merge" && (!next || next.start - current.end > 2 || next.start - current.end < -.005)) {
      setNote("Chỉ gộp hai câu liền nhau không chồng mốc, cách nhau tối đa 2 giây."); return;
    }
    const fraction = (at - current.start) / (current.end - current.start);
    const [leftSource, rightSource] = kind === "split" ? splitSuggestion(current.source, fraction) : [current.source + " " + (next?.source ?? ""), ""];
    const [leftText, rightText] = kind === "split" ? splitSuggestion(current.translation, fraction) : [current.translation + " " + (next?.translation ?? ""), ""];
    setForm({ kind, at: Number.isFinite(at) ? at.toFixed(3) : "", leftSource, rightSource, leftText, rightText, reviewed: false });
  }

  function submit() {
    if (!form || blocked || !form.reviewed || !current?.id) return;
    if (form.kind === "undo") { onApply({ kind: "undo", revision }); return; }
    const problem = structureTextProblem(form.leftSource, form.leftText, ...(form.kind === "split" ? [form.rightSource, form.rightText] : []));
    if (problem) { setNote(problem); return; }
    if (form.kind === "split") {
      const at = form.at.trim() ? Number(form.at) : NaN;
      if (!Number.isFinite(at) || at - current.start < .079999 || current.end - at < .079999) { setNote("Điểm tách phải nằm trong câu, mỗi phần dài ít nhất 0,08 giây."); return; }
      onApply({ kind: "split", revision, id: current.id, at, left_source: form.leftSource, left_text: form.leftText, right_source: form.rightSource, right_text: form.rightText });
    } else if (next?.id) {
      onApply({ kind: "merge", revision, id: current.id, next_id: next.id, source: form.leftSource, text: form.leftText });
    }
  }

  return <div className="structure-editor">
    <div className="timing-actions">
      <button type="button" className="link" disabled={blocked} onClick={() => open("split")}>Tách tại vị trí phát</button>
      <button type="button" className="link" disabled={blocked || !next} onClick={() => open("merge")}>Gộp với câu sau</button>
      <button type="button" className="link" disabled={blocked || !current?.structure_undo} onClick={() => open("undo")}>Hoàn tác tách/gộp</button>
    </div>
    {pending ? <p className="timing-hint">Xuất hoặc Bỏ sửa các nháp lời/mốc trước khi tách/gộp.</p> : null}
    {current?.structure_undo_blocked ? <p className="timing-hint">Câu trong thao tác gần nhất đã sửa tiếp. Giữ lời/mốc mới, không tự hoàn tác đè lên.</p> : null}
    {form ? <div className="structure-review" ref={reviewRef}>
      <strong>{form.kind === "split" ? "Xem lại hai phần câu" : form.kind === "merge" ? `Gộp câu ${selected + 1} và ${selected + 2}` : "Hoàn tác tách/gộp gần nhất"}</strong>
      <p className="timing-hint">{form.kind === "split"
        ? "Chữ được gợi ý chia theo thời gian, chưa căn từ theo giọng. Kiểm tra và sửa cả hai phần trước khi xuất."
        : form.kind === "merge"
          ? `Khoảng câu mới giữ cả khoảng nghỉ ${Math.max(0, (next?.start ?? 0) - current.end).toFixed(2)} giây. App chưa nhận diện người nói; chỉ gộp khi nội dung và lượt nói phù hợp.`
          : "Khôi phục cấu trúc trước thao tác gần nhất và xuất lại bằng tiếng đã giữ. Nếu chính câu đó đã sửa tiếp, app sẽ báo để tránh ghi đè lời/mốc mới. Các câu khác giữ nguyên."}</p>
      {form.kind === "split" ? <label>Điểm tách (giây)<input type="number" step="0.001" value={form.at} disabled={blocked} onChange={e => setForm({ ...form, at: e.target.value, reviewed: false })} /></label> : null}
      {form.kind !== "undo" ? <div className="structure-parts">
        <fieldset disabled={blocked}><legend>{form.kind === "split" ? "Phần đầu" : "Câu sau khi gộp"}</legend>
          <label>Lời gốc<textarea aria-label={form.kind === "split" ? "Lời gốc phần đầu" : "Lời gốc sau khi gộp"} rows={2} value={form.leftSource} onChange={e => setForm({ ...form, leftSource: e.target.value })} /></label>
          <label>Lời đọc<textarea aria-label={form.kind === "split" ? "Lời đọc phần đầu" : "Lời đọc sau khi gộp"} rows={2} value={form.leftText} onChange={e => setForm({ ...form, leftText: e.target.value })} /></label>
        </fieldset>
        {form.kind === "split" ? <fieldset disabled={blocked}><legend>Phần sau</legend>
          <label>Lời gốc<textarea aria-label="Lời gốc phần sau" rows={2} value={form.rightSource} onChange={e => setForm({ ...form, rightSource: e.target.value })} /></label>
          <label>Lời đọc<textarea aria-label="Lời đọc phần sau" rows={2} value={form.rightText} onChange={e => setForm({ ...form, rightText: e.target.value })} /></label>
        </fieldset> : null}
      </div> : null}
      <label className="structure-confirm"><input type="checkbox" checked={form.reviewed} disabled={blocked} onChange={e => setForm({ ...form, reviewed: e.target.checked })} />Tôi đã xem lại {form.kind === "undo" ? "thao tác cần hoàn tác" : "lời và mốc câu"}</label>
      <div className="timing-actions">
        <button type="button" className="link" disabled={blocked} onClick={() => { setForm(null); setNote(""); }}>Hủy tách/gộp</button>
        <button type="button" className="check" disabled={blocked || !form.reviewed} onClick={submit}>{form.kind === "split" ? "Tách và xuất lại" : form.kind === "merge" ? "Gộp và xuất lại" : "Hoàn tác và xuất lại"}</button>
      </div>
    </div> : null}
    {note ? <p className="error" role="status">{note}</p> : null}
  </div>;
}
