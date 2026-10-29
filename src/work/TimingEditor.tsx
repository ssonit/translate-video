import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState, type PointerEvent, type RefObject } from "react";
import { errorText, pickedEdits } from "./format";
import { StructureEditor } from "./StructureEditor";
import { cueTimes, loopRange, movedTimes, timingProblem } from "./timing";
import type { LinePick, ScriptLine, StructureEdit } from "./types";
import "./TimingEditor.css";

type TimelineData = { duration: number; revision: string; peaks: number[]; waveformSource: string; note: string };
type Picks = Record<number, LinePick>;
type UndoTime = { index: number; start?: number; end?: number; revision?: string };
type Drag = { id: number; x: number; width: number; span: number; start: number; end: number; mode: "move" | "start" | "end" };

export function TimingEditor({ path, lines, picks, onPicks, videoRef, playable, busy, onStructure, selectedIndex, onSelected }: {
  path: string; lines: ScriptLine[]; picks: Picks; onPicks: (next: Picks) => void;
  videoRef: RefObject<HTMLMediaElement | null>; playable: boolean; busy: boolean;
  onStructure: (action: StructureEdit) => void;
  selectedIndex: number; onSelected: (index: number) => void;
}) {
  const [open, setOpen] = useState(false);
  const [data, setData] = useState<TimelineData | null>(null);
  const [loading, setLoading] = useState(false);
  const [waveLoading, setWaveLoading] = useState(false);
  const [note, setNote] = useState("");
  const selected = Math.min(selectedIndex, Math.max(0, lines.length - 1));
  const setSelected = onSelected;
  const [from, setFrom] = useState(0);
  const [span, setSpan] = useState(20);
  const [startText, setStartText] = useState("");
  const [endText, setEndText] = useState("");
  const [ghost, setGhost] = useState<{ index: number; start: number; end: number } | null>(null);
  const [looping, setLooping] = useState(false);
  const [undo, setUndo] = useState<UndoTime[]>([]);
  const [cursor, setCursor] = useState(0);
  const drag = useRef<Drag | null>(null);
  const loopRequest = useRef(0);
  const track = useRef<HTMLDivElement>(null);
  const current = lines[selected];
  const duration = data?.duration ?? 0;
  const width = Math.min(span, duration || span);
  const left = Math.max(0, Math.min(from, duration - width));
  const times = current ? cueTimes(current, picks[selected]) : { start: 0, end: 0 };
  const stale = data !== null && (!current?.revision || current.revision !== data.revision);
  const disabled = busy || !data || stale;
  const canLoop = [times.start, times.end].every(Number.isFinite) && times.start >= 0 && times.end > times.start && times.end <= duration + 0.001;

  useEffect(() => {
    setOpen(false); setData(null); setUndo([]); setSelected(0); setNote(""); setGhost(null); setLooping(false);
    drag.current = null;
    loopRequest.current += 1;
  }, [path, lines]);

  useEffect(() => {
    if (!open || busy) return;
    let cancelled = false;
    setLoading(true); setNote(""); setWaveLoading(false);
    invoke<TimelineData>("timeline_data", { path, waveform: false }).then(async info => {
      if (cancelled) return;
      setData(info); setLoading(false); setWaveLoading(true);
      try {
        const wave = await invoke<TimelineData>("timeline_data", { path, waveform: true });
        if (!cancelled) setData(wave);
      } catch (error) { if (!cancelled) setNote(errorText(error)); }
      finally { if (!cancelled) setWaveLoading(false); }
    }).catch(error => { if (!cancelled) { setNote(errorText(error)); setLoading(false); } });
    return () => { cancelled = true; };
  }, [open, path, busy]);

  useEffect(() => {
    loopRequest.current += 1;
    setStartText(times.start.toFixed(3)); setEndText(times.end.toFixed(3)); setLooping(false);
  }, [selected, times.start, times.end]);

  useEffect(() => {
    if (times.start < left || times.start >= left + width) setFrom(Math.max(0, times.start - span / 4));
    // Follow a newly selected script line, while allowing manual timeline panning.
  }, [selected]);

  useEffect(() => { if (Object.keys(picks).length === 0) setUndo([]); }, [picks]);

  useEffect(() => {
    if (!open) return;
    const node = videoRef.current;
    if (!node) return;
    const sync = () => setCursor(node.currentTime);
    node.addEventListener("timeupdate", sync);
    return () => node.removeEventListener("timeupdate", sync);
  }, [open, videoRef]);

  useEffect(() => {
    if (!looping || !open || busy || !playable || !current || !data) return;
    const node = videoRef.current;
    if (!node) return;
    const range = loopRange(times.start, times.end, duration);
    let frame = 0;
    let alive = true;
    const tick = () => {
      if (!alive) return;
      if (node.currentTime >= range.end || node.currentTime < range.start - 0.1) {
        node.currentTime = range.start;
        void node.play().catch(() => { if (alive) setLooping(false); });
      }
      setCursor(node.currentTime);
      frame = requestAnimationFrame(tick);
    };
    const pause = () => { if (!node.ended) { loopRequest.current += 1; setLooping(false); } };
    node.addEventListener("pause", pause);
    frame = requestAnimationFrame(tick);
    return () => { alive = false; cancelAnimationFrame(frame); node.removeEventListener("pause", pause); };
  }, [looping, open, busy, playable, current, times.start, times.end, duration, data, videoRef]);

  function apply(index: number, start: number, end: number) {
    if (disabled || !data) return;
    const problem = timingProblem(lines, picks, index, start, end, duration);
    setNote(problem);
    if (problem) return;
    const previous = cueTimes(lines[index], picks[index]);
    if (start === previous.start && end === previous.end) return;
    const next = { ...picks, [index]: { ...(picks[index] ?? { text: lines[index].translation, again: false }), start, end, revision: data.revision } };
    setUndo(history => [...history.slice(-19), { index, start: picks[index]?.start, end: picks[index]?.end, revision: picks[index]?.revision }]);
    onPicks(next);
  }

  function select(index: number) {
    loopRequest.current += 1;
    setSelected(index); setNote(""); setLooping(false);
    const cue = cueTimes(lines[index], picks[index]);
    setFrom(Math.max(0, cue.start - span / 4));
    const node = videoRef.current;
    if (node && playable && Number.isFinite(cue.start) && cue.start >= 0) { node.currentTime = cue.start; setCursor(cue.start); }
  }

  function begin(event: PointerEvent<HTMLButtonElement>, index: number, mode: Drag["mode"]) {
    event.stopPropagation();
    if (disabled || !track.current) return;
    setLooping(false); setSelected(index); setNote("");
    loopRequest.current += 1;
    const cue = cueTimes(lines[index], picks[index]);
    drag.current = { id: index, x: event.clientX, width: track.current.getBoundingClientRect().width, span: width, ...cue, mode };
    event.currentTarget.setPointerCapture(event.pointerId);
  }

  function move(event: PointerEvent<HTMLButtonElement>) {
    const state = drag.current;
    if (!state || disabled) return;
    const delta = (event.clientX - state.x) / state.width * state.span;
    setGhost({ index: state.id, ...movedTimes(state.start, state.end, delta, state.mode, duration) });
  }

  function finish(event: PointerEvent<HTMLButtonElement>) {
    const state = drag.current;
    if (!state) return;
    const delta = (event.clientX - state.x) / state.width * state.span;
    const next = movedTimes(state.start, state.end, delta, state.mode, duration);
    drag.current = null; setGhost(null);
    if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
    if (Math.abs(delta) > 0.0005) apply(state.id, next.start, next.end);
  }

  function loop() {
    if (looping) { loopRequest.current += 1; setLooping(false); videoRef.current?.pause(); return; }
    const node = videoRef.current;
    if (!node || !data || !canLoop) return;
    const request = ++loopRequest.current;
    setNote("");
    node.currentTime = loopRange(times.start, times.end, duration).start;
    setLooping(true);
    void node.play().catch(() => {
      if (loopRequest.current === request) { setLooping(false); setNote("Chưa phát được đoạn này."); }
    });
  }

  const wavePath = data?.peaks.map((peak, index) => {
    const at = (index + 0.5) / data.peaks.length * duration;
    if (at < left || at > left + width) return "";
    const x = (at - left) / width * 1000;
    const amplitude = Math.max(0.7, Math.sqrt(peak) * 21);
    return `M${x.toFixed(1)},${(24 - amplitude).toFixed(1)}V${(24 + amplitude).toFixed(1)}`;
  }).join("") ?? "";

  return <div className="timing-editor">
    <button type="button" className="link timing-toggle" aria-expanded={open} disabled={busy} onClick={() => { loopRequest.current += 1; setOpen(!open); setLooping(false); }}>
      {open ? "Thu gọn timeline" : "Mở timeline · Chỉnh thời gian câu"}
    </button>
    {open ? <div className="timing-panel">
      <p className="timing-hint">Đang xem bản xuất hiện tại. Mốc nháp chỉ đổi hình và tiếng sau khi bấm Xuất lại.</p>
      {loading ? <p role="status">Đang đọc mốc video…</p> : null}
      {data ? <>
        <label className="timing-select">Câu <select aria-label="Chọn câu trên timeline" value={selected} disabled={disabled} onChange={e => select(Number(e.target.value))}>
          {lines.map((line, index) => <option key={index} value={index}>{index + 1}. {(picks[index]?.text ?? line.translation).slice(0, 65)}</option>)}
        </select></label>
        <div className="timing-zoom">
          <button type="button" className="link" disabled={left <= 0} onClick={() => setFrom(Math.max(0, left - width / 2))}>←</button>
          <span>{left.toFixed(1)}–{(left + width).toFixed(1)} giây</span>
          <select aria-label="Khoảng thời gian hiển thị" value={span} onChange={e => { setSpan(Number(e.target.value)); setFrom(Math.max(0, times.start - Number(e.target.value) / 4)); }}>
            {[10, 20, 60].map(n => <option key={n} value={n}>{n} giây</option>)}
          </select>
          <button type="button" className="link" disabled={left + width >= duration} onClick={() => setFrom(Math.min(duration - width, left + width / 2))}>→</button>
        </div>
        <div className="timing-track" ref={track} onPointerDown={event => {
          if (event.target instanceof Element && event.target.closest("button")) return;
          const node = videoRef.current;
          if (!node || !playable) return;
          setLooping(false);
          node.currentTime = Math.max(0, Math.min(duration, left + (event.clientX - event.currentTarget.getBoundingClientRect().left) / event.currentTarget.getBoundingClientRect().width * width));
          setCursor(node.currentTime);
        }}>
          <svg viewBox="0 0 1000 48" preserveAspectRatio="none" aria-hidden="true"><path d={wavePath} /></svg>
          <span className="timing-cursor" style={{ left: `${(cursor - left) / width * 100}%` }} />
          {lines.map((line, index) => {
            const cue = ghost?.index === index ? ghost : cueTimes(line, picks[index]);
            if (cue.end <= left || cue.start >= left + width) return null;
            const pointer = { onPointerMove: move, onPointerUp: finish, onPointerCancel: () => { drag.current = null; setGhost(null); } };
            return <div key={index} className={`timing-cue ${selected === index ? "is-selected" : ""}`} style={{ left: `${Math.max(0, (cue.start - left) / width * 100)}%`, width: `${Math.max(0.2, (Math.min(cue.end, left + width) - Math.max(cue.start, left)) / width * 100)}%` }}>
              <button type="button" className="timing-handle" aria-label={`Đầu câu ${index + 1}`} disabled={disabled} onPointerDown={e => begin(e, index, "start")} {...pointer} />
              <button type="button" className="timing-body" aria-label={`Di chuyển câu ${index + 1}`} disabled={disabled} onClick={() => setSelected(index)} onPointerDown={e => begin(e, index, "move")} {...pointer}>{index + 1}</button>
              <button type="button" className="timing-handle" aria-label={`Cuối câu ${index + 1}`} disabled={disabled} onPointerDown={e => begin(e, index, "end")} {...pointer} />
            </div>;
          })}
        </div>
        <small>{waveLoading ? "Đang lấy sóng âm…" : data.peaks.length ? `Sóng âm ${data.waveformSource}` : "Chưa có sóng âm"}</small>
        <div className="timing-fields">
          <label>Bắt đầu (giây)<input type="number" step="0.001" min="0" max={duration} value={startText} disabled={disabled} onChange={e => setStartText(e.target.value)} /></label>
          <label>Kết thúc (giây)<input type="number" step="0.001" min="0" max={duration} value={endText} disabled={disabled} onChange={e => setEndText(e.target.value)} /></label>
          <button type="button" className="check" disabled={disabled} onClick={() => apply(selected, startText.trim() ? Number(startText) : NaN, endText.trim() ? Number(endText) : NaN)}>Lưu mốc nháp</button>
        </div>
        <div className="timing-actions">
          <button type="button" className="link" disabled={disabled || !playable || !canLoop} onClick={loop}>{looping ? "Dừng lặp" : "Nghe lặp"}</button>
          <button type="button" className="link" disabled={disabled || !undo.length} onClick={() => {
            const previous = undo[undo.length - 1];
            if (previous) onPicks({ ...picks, [previous.index]: { ...(picks[previous.index] ?? { text: lines[previous.index].translation, again: false }), start: previous.start, end: previous.end, revision: previous.revision } });
            setUndo(history => history.slice(0, -1)); setNote("");
          }}>Hoàn tác mốc</button>
        </div>
        <details className="structure-tools"><summary>Tách / gộp câu</summary><StructureEditor lines={lines} selected={selected} revision={data.revision} disabled={disabled} pending={pickedEdits(lines, picks).length > 0} videoRef={videoRef} onApply={onStructure} /></details>
        {data.note ? <p className="timing-hint">{data.note}</p> : null}
        {stale ? <p className="error">Dữ liệu đã thay đổi. Mở lại video từ thư viện trước khi sửa tiếp.</p> : null}
      </> : null}
      {note ? <p className="error" role="status">{note}</p> : null}
    </div> : null}
  </div>;
}
