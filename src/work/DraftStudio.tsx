import { useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { DeepseekServerRow, useDeepseekServer } from "../DeepseekServer";
import { LockedNote } from "../SetupChip";
import { lockNote, summarize, type SetupSummary, type ToolStatus } from "../setupStatus";
import { StyleEditor } from "./StyleEditor";
import { LANGUAGES, errorText, fileName } from "./format";
import { MODEL_HINTS } from "./models";
import { LOOK_AHEAD_SECONDS, missingRegion, playbackInterrupted, regionQueue, splitText, type DraftClip, type DraftCue, type DraftOptions, type DraftPatch, type DraftRegion, type DraftView } from "./demand";
import type { StructureEdit } from "./types";
import "./DraftStudio.css";

type Mode = "source" | "text" | "voice";
type SplitForm = { kind: "split" | "merge"; at: number; leftSource: string; rightSource: string; leftText: string; rightText: string };
const clockText = (t: number) => `${Math.floor(t / 60)}:${Math.floor(t % 60).toString().padStart(2, "0")}`;

export function DraftStudio({ root, initial, active, lock: globalLock, sourceLock, onTools, onActivity, onBack, onResult, onChanged, onGuard, onSetupSummary }: {
  root: string; initial: DraftView | null; active: boolean; lock: string; sourceLock: string;
  onTools: () => void; onActivity: (label: string) => void; onBack: () => void; onResult: (path: string, textOnly?: boolean) => void; onChanged: () => void;
  onGuard: (guard: (() => Promise<boolean>) | null) => void;
  onSetupSummary: (summary: SetupSummary | null) => void;
}) {
  const [view, setView] = useState<DraftView | null>(initial);
  const [config, setConfig] = useState<DraftOptions | null>(initial?.options ?? null);
  const [mode, setMode] = useState<Mode>("source");
  const [patches, setPatches] = useState<Record<string, DraftPatch>>({});
  const [selected, setSelected] = useState(initial?.cues[0]?.id ?? "");
  const [rangeBusy, setRangeBusy] = useState(false);
  const [operation, setOperation] = useState("");
  const [detail, setDetail] = useState("");
  const [note, setNote] = useState("");
  const [error, setError] = useState("");
  const [time, setTime] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [waiting, setWaiting] = useState(false);
  const [speed, setSpeed] = useState(1);
  const [ratio, setRatio] = useState(16 / 9);
  const [width, setWidth] = useState(600);
  const [logo, setLogo] = useState("");
  const [voices, setVoices] = useState<string[]>([]);
  const [key, setKey] = useState("");
  const [sample, setSample] = useState("");
  const [split, setSplit] = useState<SplitForm | null>(null);
  const [query, setQuery] = useState("");
  const [localSetup, setLocalSetup] = useState<SetupSummary | null>(null);
  const [setupEpoch, setSetupEpoch] = useState(0);
  const lock = localSetup ? lockNote(localSetup) : "";
  function changed() { setSetupEpoch(value => value + 1); onChanged(); }
  const media = useRef<HTMLVideoElement | null>(null);
  const backdrop = useRef<HTMLVideoElement | null>(null);
  const audio = useRef<HTMLAudioElement | null>(null);
  const box = useRef<HTMLDivElement | null>(null);
  const alive = useRef(true);
  const state = useRef({ view, config, patches, mode, speed, active, operation, clips: [] as DraftClip[], desired: false, cursor: 0, singleEnd: null as number | null });
  state.current = { ...state.current, view, config, patches, mode, speed, active, operation };
  const lastFrame = useRef(0);
  const lastUi = useRef(0);
  const currentAudio = useRef("");
  const requested = useRef("");
  const prefix = useRef("");
  const opening = useRef<Promise<DraftView> | null>(null);
  const deepseek = useDeepseekServer(config?.provider === "deepseek-local" && !config.skipTranslate);
  const configDirty = !!view && !!config && JSON.stringify(config) !== JSON.stringify(view.options);
  const dirty = Object.keys(patches).length > 0 || configDirty;
  const busy = operation !== "";

  function adopt(next: DraftView, keepConfig = true) {
    if (!alive.current || next.root !== root) return;
    const old = state.current;
    const keep = keepConfig && old.config && old.view && JSON.stringify(old.config) !== JSON.stringify(old.view.options);
    state.current.view = next;
    setView(next);
    if (!keep) { state.current.config = next.options; setConfig(next.options); }
    setSelected(id => next.cues.some(c => c.id === id) ? id : next.cues[0]?.id ?? "");
  }
  const queue = useRef<ReturnType<typeof regionQueue<DraftRegion>> | null>(null);
  function makeQueue() { const owner = crypto.randomUUID(); prefix.current = owner; return regionQueue<DraftRegion>({
    publish: intent => invoke("set_draft_region", { intent, owner }),
    run: (root, serial) => invoke<DraftRegion>("prepare_draft_region", { root, serial }),
    accept: result => {
      state.current.clips = result.clips;
      adopt(result.view);
      requested.current = "";
    },
    fail: caught => { if (alive.current) { setError(errorText(caught)); state.current.desired = false; setPlaying(false); } },
    busy: value => { if (alive.current) setRangeBusy(value); },
    cancel: (root, serial) => invoke("cancel_draft_region", { root, serial }),
  }, owner); }
  if (!queue.current) queue.current = makeQueue();

  useEffect(() => { onActivity(operation || (rangeBusy ? "Đang chuẩn bị vùng xem/nghe" : "")); }, [operation, rangeBusy, onActivity]);
  useEffect(() => {
    let live = true;
    if (config) void invoke<ToolStatus[]>("check_setup", { options: config }).then(tools => {
      if (!live) return; const next=summarize(tools,false);setLocalSetup(next);onSetupSummary(next);
    }).catch(() => { if (live) { const next: SetupSummary={kind:"failed",missing:[]};setLocalSetup(next);onSetupSummary(next); } });
    return () => { live=false; };
    // Style changes do not affect tool availability.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [config?.provider, config?.model, config?.skipTranslate, config?.target, config?.voice, config?.textOnly, setupEpoch, globalLock, onSetupSummary]);
  useEffect(() => () => onSetupSummary(null), [onSetupSummary]);
  useEffect(() => {
    onGuard(async () => {
      if (state.current.operation) return false;
      pause(); await queue.current?.idle();
      const s = state.current;
      return Object.keys(s.patches).length || JSON.stringify(s.config) !== JSON.stringify(s.view?.options) ? saveAll() : true;
    });
    return () => onGuard(null);
    // The guard reads current refs, including changes made while hidden.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [onGuard]);
  useEffect(() => {
    alive.current = true;
    if (!queue.current) queue.current = makeQueue();
    let live = true;
    const owner = prefix.current;
    const subscription = listen<DraftRegion & {serial: string}>("draft-region-progress", event => {
      if (!live || !event.payload.serial.startsWith(`${owner}:`) || event.payload.view.root !== root) return;
      state.current.clips = event.payload.clips; adopt(event.payload.view);
    });
    if (!initial) {
      setOperation("Đang mở nháp…");
      opening.current ??= invoke<DraftView>("read_draft", { root });
      void opening.current.then(v => { if (live) adopt(v, false); }).catch(e => { if (live) setError(errorText(e)); }).finally(() => { if (live) setOperation(""); });
    }
    return () => { live = false; void subscription.then(stop => stop()); alive.current = false; state.current.desired = false; media.current?.pause(); audio.current?.pause(); const old = queue.current; queue.current = null; void old?.close(); };
    // A source owns its queue and callbacks for its entire mounted lifetime.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [root]);
  useEffect(() => {
    if (!active) pause();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active]);
  useEffect(() => {
    if (!box.current) return;
    const observer = new ResizeObserver(([entry]) => setWidth(entry.contentRect.width));
    observer.observe(box.current); return () => observer.disconnect();
  }, [view?.source]);
  useEffect(() => {
    let live = true;
    if (config) void invoke<string[]>("list_voices", { lang: config.target }).then(list => { if (live) setVoices(list); }).catch(() => {});
    return () => { live = false; };
  }, [config?.target]);
  useEffect(() => {
    let live = true; setLogo("");
    const item = config?.render.logo;
    if (item?.enabled) void invoke("allow_logo_preview", { path: item.path, sha256: item.sha256 }).then(() => { if (live) setLogo(convertFileSrc(item.path)); }).catch(e => setError(errorText(e)));
    return () => { live = false; };
  }, [config?.render.logo]);

  function pause() {
    state.current.desired = false;
    media.current?.pause(); backdrop.current?.pause(); audio.current?.pause();
    setPlaying(false); setWaiting(false);
    void queue.current?.pause().catch(e => setError(errorText(e)));
  }
  function seek(next: number) {
    const duration = state.current.view?.duration ?? 0;
    state.current.cursor = Math.max(0, Math.min(duration, next));
    state.current.singleEnd = null;
    if (media.current) media.current.currentTime = state.current.cursor;
    if (backdrop.current) backdrop.current.currentTime = state.current.cursor;
    currentAudio.current = ""; audio.current?.pause(); setTime(state.current.cursor); requested.current = "";
    if (state.current.desired && state.current.mode !== "source") demand(state.current.cursor, true);
  }
  function demand(at: number, force = false, cue?: string) {
    const s = state.current;
    if (!s.view || s.mode === "source" || s.operation || (!force && !missingRegion(s.view, at, s.mode === "voice"))) return;
    const end = Math.min(s.view.duration, at + LOOK_AHEAD_SECONDS);
    if (end <= at) return;
    const token = `${s.mode}:${cue ?? ""}:${Math.floor(at / 3)}`;
    if (!force && requested.current === token) return;
    requested.current = token;
    queue.current?.request({ root, start: at, end, listen: s.mode === "voice", cue });
  }
  async function chooseMode(next: Mode) {
    if (next !== "source" && dirty && !await saveAll()) return;
    pause(); state.current.mode = next; state.current.clips = []; setMode(next); currentAudio.current = ""; requested.current = ""; setError("");
    if (next !== "source") demand(state.current.cursor, true);
  }
  async function toggle() {
    if (state.current.desired) { pause(); return; }
    if (state.current.operation) return;
    if (state.current.mode !== "source" && (lock || (dirty && !await saveAll()))) return;
    const s = state.current;
    if (!s.view || (s.mode === "source" && !s.view.source)) return;
    if (s.mode === "voice" && s.config?.textOnly) {s.mode="text";setMode("text");}
    state.current.desired = true; state.current.singleEnd = null; setPlaying(true); setError("");
    if (s.mode !== "source") demand(state.current.cursor, true);
    lastFrame.current = performance.now();
  }

  useEffect(() => {
    let frame = 0;
    function tick(now: number) {
      const s = state.current;
      if (s.view && s.config && s.active && s.desired && !s.operation) {
        const delta = Math.min(0.15, (now - (lastFrame.current || now)) / 1000);
        const node = media.current;
        const t = node && s.view.source ? node.currentTime : s.cursor;
        s.cursor = t;
        const cue = s.view.cues.find(c => c.line.start <= t && c.line.end > t);
        const clip = s.clips.find(c => c.start <= t && c.start + c.played > t);
        const noWords = !s.view.cues.length || (!!cue && !cue.line.translation.trim());
        const hold = s.mode !== "source" && (noWords || (s.mode === "voice" && !!cue && (!s.view.ready.includes(cue.id) || !s.clips.some(c => c.id === cue.id))));
        if (hold) {
          node?.pause(); backdrop.current?.pause(); audio.current?.pause(); setWaiting(true);
          demand(t);
        } else {
          setWaiting(false);
          if (node && s.view.source) {
            node.playbackRate = s.speed;
            node.volume = s.mode === "voice" ? Math.min(1, s.config.render.audio.original / 100 * (clip && s.config.render.audio.duck ? s.config.render.audio.duckLevel / 100 : 1)) : 1;
            if (node.paused && !node.ended) void node.play().catch(e => { if (!alive.current || !s.desired || playbackInterrupted(e)) return; s.desired = false; setPlaying(false); setError(errorText(e)); });
            if (backdrop.current) {
              backdrop.current.playbackRate = s.speed;
              if (Math.abs(backdrop.current.currentTime - t) > 0.2) backdrop.current.currentTime = t;
              if (backdrop.current.paused) void backdrop.current.play().catch(() => {});
            }
          } else s.cursor += delta * s.speed;
          if (s.mode === "voice" && clip && audio.current) {
            const a = audio.current; const key = `${clip.id}:${clip.path}:${clip.version}`;
            const offset = Math.max(0, (t - clip.start) * clip.speed);
            if (currentAudio.current !== key) { a.pause(); a.src = `${convertFileSrc(clip.path)}?v=${encodeURIComponent(clip.version)}`; a.currentTime = offset; currentAudio.current = key; }
            else if (Math.abs(a.currentTime - offset) > 0.25) a.currentTime = offset;
            a.playbackRate = clip.speed * s.speed; a.volume = Math.min(1, s.config.render.audio.voice / 100);
            if (a.paused) void a.play().catch(e => { if (!alive.current || !s.desired || playbackInterrupted(e)) return; s.desired = false; setPlaying(false); setError(errorText(e)); });
          } else { audio.current?.pause(); currentAudio.current = ""; }
          if (!s.singleEnd && s.mode !== "source") demand(t);
          if (s.cursor >= s.view.duration - 0.02 || (s.singleEnd !== null && s.cursor >= s.singleEnd - 0.02)) pause();
        }
        if (now - lastUi.current > 100) { setTime(s.cursor); lastUi.current = now; }
      }
      lastFrame.current = now; frame = requestAnimationFrame(tick);
    }
    frame = requestAnimationFrame(tick); return () => cancelAnimationFrame(frame);
    // refs intentionally carry the current source, configuration and playback intent.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function exclusive<T>(label: string, work: () => Promise<T>): Promise<T | undefined> {
    if (state.current.operation) return;
    pause(); state.current.operation = label; setOperation(label); setError(""); setNote("");
    try { await queue.current?.pause(); await queue.current?.idle(); return await work(); }
    catch (e) { if (alive.current) setError(errorText(e)); return undefined; }
    finally { state.current.operation = ""; if (alive.current) setOperation(""); }
  }
  async function saveAll() {
    const result = await exclusive("Đang lưu nháp…", async () => {
      const localPatches = Object.values(state.current.patches);
      const desired = state.current.config;
      if (localPatches.some(p => (p.start !== undefined && !Number.isFinite(p.start)) || (p.end !== undefined && !Number.isFinite(p.end)))) throw Error("Điền mốc bắt đầu và kết thúc hợp lệ.");
      if (localPatches.length) {
        const next = await invoke<DraftView>("save_draft", { root, revision: state.current.view!.revision, patches: localPatches });
        state.current.patches = {}; setPatches({}); adopt(next);
      }
      if (desired && JSON.stringify(desired) !== JSON.stringify(state.current.view!.options)) {
        const next = await invoke<DraftView>("configure_draft", { root, revision: state.current.view!.revision, options: desired });
        adopt(next, false);
        await invoke("switch_translation_provider", { provider: desired.provider });
        await invoke("save_translation_model", { model: desired.model });
        if (desired.voice && !desired.textOnly) await invoke("save_voice", { lang: desired.target, name: desired.voice });
        await invoke("save_prefs", { useTerms: desired.useTerms, skipTranslate: desired.skipTranslate });
        changed();
      }
      setNote("Đã lưu nháp trên máy. Bản xuất trước vẫn giữ nguyên."); return true;
    });
    return result === true;
  }
  async function prepare() {
    await exclusive("Đang lấy lời gốc…", async () => adopt(await invoke<DraftView>("prepare_draft", { root })));
  }
  async function hear(cue: DraftCue) {
    if (dirty && !await saveAll()) return;
    pause(); setMode("voice"); state.current.mode = "voice"; setSelected(cue.id); seek(cue.line.start);
    await queue.current?.pause(); await queue.current?.idle();
    demand(cue.line.start, true, cue.id);
    await queue.current?.idle();
    const clip = state.current.clips.find(c => c.id === cue.id);
    if (!clip) return;
    seek(clip.start); state.current.singleEnd = Math.min(state.current.view!.duration, clip.start + clip.played);
    state.current.desired = true; setPlaying(true); lastFrame.current = performance.now();
  }
  async function exportVideo() {
    if (dirty && !await saveAll()) return;
    const path = await exclusive("Đang xuất bản đầy đủ…", async () => {
      const path = await invoke<string>("export_draft", { root });
      adopt(await invoke<DraftView>("read_draft", { root }), false); return path;
    });
    if (path) onResult(path, state.current.config?.textOnly ?? false);
  }
  async function sampleVideo() {
    if (dirty && !await saveAll()) return;
    await exclusive("Đang dựng đoạn thử…", async () => {
      const path = await invoke<string>("sample_draft", { root, start: state.current.cursor, seconds: Math.min(8, Math.max(1, (view?.duration ?? 1) - state.current.cursor)) });
      setSample(convertFileSrc(path)); adopt(await invoke<DraftView>("read_draft", { root }));
    });
  }
  async function undo() {
    await exclusive("Đang hoàn tác…", async () => {
      adopt(await invoke<DraftView>("structure_draft", { root, revision: state.current.view!.revision, action: null }));
      state.current.patches = {}; setPatches({}); setSplit(null);
    });
  }
  async function splitOrMerge() {
    if (!split || !current) return;
    if (dirty && !await saveAll()) return;
    await exclusive("Đang lưu cấu trúc câu…", async () => {
      const latest = state.current.view!;
      const index = latest.cues.findIndex(c => c.id === current.id);
      const action: StructureEdit = split.kind === "split" ? {
        kind: "split", revision: latest.revision, id: current.id, at: split.at,
        left_source: split.leftSource, right_source: split.rightSource, left_text: split.leftText, right_text: split.rightText,
      } : { kind: "merge", revision: latest.revision, id: current.id, next_id: latest.cues[index + 1].id, source: split.leftSource, text: split.leftText };
      adopt(await invoke<DraftView>("structure_draft", { root, revision: latest.revision, action })); setSplit(null);
    });
  }
  async function leave() {
    if (dirty && !await saveAll()) return;
    pause(); await queue.current?.close(); onBack();
  }
  async function stop() {
    pause(); setDetail("Đang dừng, chờ bộ xử lý hiện tại kết thúc.");
    await invoke("cancel_dub");
  }
  function patch(values: Partial<DraftPatch>) {
    if (!current) return; pause();
    setPatches(previous => { const next = { ...previous, [current.id]: { ...previous[current.id], id: current.id, ...values } }; state.current.patches = next; return next; });
  }
  function configPatch(values: Partial<DraftOptions>) { pause(); setConfig(previous => { const next = previous ? { ...previous, ...values } : previous; state.current.config = next; return next; }); }
  const index = view?.cues.findIndex(c => c.id === selected) ?? -1;
  const current = view?.cues[index];
  const edited = current ? patches[current.id] : undefined;
  const visible = useMemo(() => {
    const all = view?.cues ?? [];
    if (query.trim()) return all.filter(c => `${c.line.source} ${c.line.translation}`.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase())).slice(0, 60);
    const page = Math.max(0, Math.floor(Math.max(0, index) / 40)) * 40;
    return all.slice(page, page + 40);
  }, [view?.cues, index, query]);
  const currentCue = view?.cues.find(c => c.line.start <= time && c.line.end > time);
  const subtitle = mode === "source" ? "" : currentCue?.line.translation;
  if (!view || !config) return <div className="draft-studio"><p role="status">{error || operation || "Đang mở nguồn…"}</p><button type="button" onClick={() => void invoke<DraftView>("read_draft", { root }).then(v => adopt(v, false)).catch(e => setError(errorText(e)))}>Nạp lại</button><button type="button" className="link" onClick={onBack}>Quay lại</button></div>;
  const render = config.render;
  const outputRatio = render.aspect === "original" ? ratio : render.aspect === "9:16" ? 9 / 16 : render.aspect === "16:9" ? 16 / 9 : 1;
  const height = width / outputRatio;
  const cover = render.aspect === "original" || render.framing === "crop";
  const imageWidth = cover ? Math.max(width, height * ratio) : Math.min(width, height * ratio);
  const imageHeight = imageWidth / ratio;
  const imageLeft = (width - imageWidth) * (cover ? render.focusX / 100 : 0.5);
  const imageTop = (height - imageHeight) * (cover ? render.focusY / 100 : 0.5);
  const fontSize = Math.min(width, height) * render.size / 100;
  const online = !["local", "deepseek-local"].includes(config.provider);
  const hint = MODEL_HINTS[config.provider];
  return <div className="draft-studio"><div className="draft-body pane-scroll">
    <header className="draft-heading"><div><p className="kicker">Bản nháp · lưu trên máy</p><h2 title={view.source}>{fileName(view.source || root)}</h2><p>Mở nguồn trước. Chỉ chuẩn bị phần đang xem/nghe; tạo file đầy đủ khi bấm Xuất.</p></div><button type="button" className="link" disabled={busy} onClick={() => void leave()}>Đổi nguồn</button></header>
    <div className="draft-grid">
      <section className="draft-preview" aria-label="Nguồn và vùng xem trước">
        <div className="draft-modes">{([["source", "Nguồn"], ["text", "Phụ đề (không đọc)"], ["voice", "Giọng lồng"]] as const).map(([id, label]) => <button key={id} type="button" aria-pressed={mode === id} disabled={busy || (id !== "source" && !!lock) || (id === "voice" && config.textOnly)} onClick={() => chooseMode(id)}>{label}</button>)}</div>
        {mode === "text" && <p className="key-note">Xem video gốc kèm phụ đề dịch, giữ tiếng gốc và không tạo hay phát giọng đọc mới. Bấm Phát để xem.</p>}
        <div className="draft-video-well"><div ref={box} className="draft-picture" style={{ aspectRatio: outputRatio, width: `min(100%, ${outputRatio * 58}vh)` }}>
          {view.source && !config.audioOnly ? <>
            {render.framing === "blur" && render.aspect !== "original" && <video ref={backdrop} src={convertFileSrc(view.source)} className="style-backdrop" muted playsInline aria-hidden="true" />}
            <video ref={media} src={convertFileSrc(view.source)} playsInline style={{ position: "absolute", left: imageLeft, top: imageTop, width: imageWidth, height: imageHeight }} onLoadedMetadata={e => setRatio(e.currentTarget.videoWidth / e.currentTarget.videoHeight || 16 / 9)} onClick={toggle} onEnded={pause} onError={() => setNote("Định dạng nguồn chưa phát được trong cửa sổ; vẫn có thể xuất qua FFmpeg.")} />
            {render.mask.enabled && <div className="draft-mask" style={{ left: imageLeft + imageWidth * render.mask.x / 100, top: imageTop + imageHeight * render.mask.y / 100, width: imageWidth * render.mask.width / 100, height: imageHeight * render.mask.height / 100, background: render.mask.mode === "solid" ? render.mask.color : "transparent", backdropFilter: render.mask.mode === "blur" ? "blur(8px)" : undefined }} />}
            {logo && <img src={logo} className="draft-logo" alt="Logo" style={{ width: `${render.logo.size}%`, opacity: render.logo.opacity / 100, left: `${render.logo.x}%`, top: `${render.logo.y}%`, transform: `translate(-${render.logo.x}%,-${render.logo.y}%)` }} />}
          </> : <div className="draft-audio-cover"><span>♪</span><strong>{config.audioOnly ? "Bản nháp audio" : "Nguồn có phụ đề"}</strong><small>{view.source ? "Chọn Giọng lồng để nghe phần đã chuẩn bị" : "Chưa tải hình hoặc tiếng gốc; đọc từ phụ đề hiện có"}</small></div>}
          {config.audioOnly && view.source && <video ref={media} src={convertFileSrc(view.source)} hidden playsInline onEnded={pause} />}
          {subtitle && <div className="draft-caption" style={{ left: `${render.captionX}%`, top: `${render.captionY}%`, fontSize, fontFamily: render.font, color: render.color, maxWidth: `${2 * (Math.min(render.captionX, 100 - render.captionX) - 7)}%`, WebkitTextStroke: `${render.outline * fontSize / 45.36}px ${render.outlineColor}`, background: render.background ? render.outlineColor : "transparent" } as CSSProperties}>{subtitle}</div>}
          {waiting && <div className="draft-holding" role="status">{mode === "voice" ? "Đang chuẩn bị giọng ở đây…" : "Đang dịch đoạn này…"}</div>}
        </div></div>
        <audio ref={audio} hidden />
        <div className="draft-player-controls"><button type="button" onClick={toggle} disabled={busy || (mode !== "source" && !!lock) || (!view.source && mode === "source")}>{playing ? "Tạm dừng" : "Phát"}</button><output>{clockText(time)} / {clockText(view.duration)}</output><input aria-label="Vị trí xem" type="range" min={0} max={view.duration} step={0.05} value={time} onChange={e => seek(+e.target.value)} /><select aria-label="Tốc độ phát" value={speed} onChange={e => setSpeed(+e.target.value)}>{[0.75, 1, 1.25, 1.5].map(v => <option key={v} value={v}>{v}×</option>)}</select></div>
        <div className="draft-status" role="status">{operation || (rangeBusy ? detail || "Đang chuẩn bị vùng yêu cầu…" : note || `${view.cues.length} câu · ${view.ready.length} câu có giọng đã xác minh`)}{view.namesReviewed ? null : <small>Tên/thuật ngữ chưa duyệt. Xuất sẽ yêu cầu kiểm tra tên và hoàn tất phần thiếu.</small>}<small>{config.audioOnly ? "Audio xuất liền mạch; mốc nghe nháp theo nguồn." : "Mốc nghe cục bộ là nháp khi thiếu ngữ cảnh; bản xuất căn toàn bộ theo cùng quy tắc."}</small></div>
        {busy || rangeBusy ? <button className="link" type="button" onClick={() => void stop()}>Dừng xử lý</button> : null}
        {error ? <p className="error" role="alert">{error}</p> : null}
        {view.warning && <p className="key-note" role="status">{view.warning}</p>}<LockedNote note={lock} onTools={onTools} />
      </section>
      <section className="draft-editor" aria-label="Lời và mốc trong nháp">
        <div className="draft-section-head"><h3>Lời thoại</h3><button type="button" className="link" disabled={busy || rangeBusy || !!sourceLock} onClick={() => void prepare()}>{view.cues.length ? "Nạp lời gốc" : "Lấy lời gốc"}</button></div>
        {!view.cues.length ? <div className="draft-empty"><strong>Chưa chép lời</strong><p>Phát nguồn ngay, hoặc lấy lời gốc để xem và chỉnh. Bước này không dịch, không tạo giọng.</p></div> : <>
          <input className="draft-search" aria-label="Tìm câu" placeholder="Tìm trong lời thoại…" value={query} onChange={e => setQuery(e.target.value)} />
          <ol className="draft-cues">{visible.map(c => <li key={c.id}><button type="button" data-selected={c.id === selected} onClick={() => { setSelected(c.id); setSplit(null); seek(c.line.start); }}><span>{clockText(c.line.start)}</span><strong>{c.line.translation || c.line.source}</strong><small>{c.authored ? "Bạn đã sửa" : view.ready.includes(c.id) ? "Có giọng" : c.line.translation ? "Có chữ" : "Lời gốc"}</small></button></li>)}</ol>
          {!query && <div className="draft-pagination"><button className="link" type="button" disabled={index < 40} onClick={() => setSelected(view.cues[Math.max(0, index - 40)].id)}>← Trước</button><small>Câu {Math.max(1, index + 1)} / {view.cues.length}</small><button className="link" type="button" disabled={index + 40 >= view.cues.length} onClick={() => setSelected(view.cues[Math.min(view.cues.length - 1, index + 40)].id)}>Sau →</button></div>}
          {current && <div className="draft-cue-edit">
            <label>Lời gốc<textarea value={edited?.source ?? current.line.source} disabled={busy} onFocus={pause} onChange={e => patch({ source: e.target.value })} /></label>
            <label>Lời đọc / bản dịch<textarea value={edited?.text ?? current.line.translation} disabled={busy} placeholder="Chưa dịch; yêu cầu xem/nghe để chuẩn bị vùng này" onFocus={pause} onChange={e => patch({ text: e.target.value })} /></label>
            <div className="draft-times"><label>Từ (giây)<input type="number" step={0.01} min={0} value={edited?.start ?? current.line.start} disabled={busy} onFocus={pause} onChange={e => patch({ start: e.target.valueAsNumber, end: edited?.end ?? current.line.end })} /></label><label>Đến (giây)<input type="number" step={0.01} min={0} value={edited?.end ?? current.line.end} disabled={busy} onFocus={pause} onChange={e => patch({ end: e.target.valueAsNumber, start: edited?.start ?? current.line.start })} /></label></div>
            <div className="draft-edit-actions"><button type="button" disabled={busy || !!lock || config.textOnly} onClick={() => void hear(current)}>Nghe câu</button><button type="button" className="link" disabled={busy || !view.canUndo || dirty} onClick={() => void undo()}>Hoàn tác</button><button type="button" className="link" disabled={busy} onClick={() => { pause(); const at = time > current.line.start + 0.08 && time < current.line.end - 0.08 ? time : (current.line.start + current.line.end) / 2; const fraction = (at - current.line.start) / (current.line.end - current.line.start); const [leftSource, rightSource] = splitText(edited?.source ?? current.line.source, fraction); const [leftText, rightText] = splitText(edited?.text ?? current.line.translation, fraction); setSplit({ kind: "split", at, leftSource, rightSource, leftText, rightText }); }}>Tách câu</button><button type="button" className="link" disabled={busy || index + 1 >= view.cues.length} onClick={() => { pause(); const next = view.cues[index + 1]; setSplit({ kind: "merge", at: 0, leftSource: `${current.line.source} ${next.line.source}`, rightSource: "", leftText: current.line.translation && next.line.translation ? `${current.line.translation} ${next.line.translation}` : "", rightText: "" }); }}>Gộp câu sau</button></div>
            {split && <fieldset className="draft-split"><legend>{split.kind === "split" ? "Xem lại hai phần trước khi tách" : "Xem lại câu gộp"}</legend>{split.kind === "split" && <label>Mốc tách<input type="number" step={0.01} value={split.at} onChange={e => setSplit({ ...split, at: e.target.valueAsNumber })} /></label>}<label>Lời gốc {split.kind === "split" ? "phần đầu" : ""}<textarea value={split.leftSource} onChange={e => setSplit({ ...split, leftSource: e.target.value })} /></label><label>Lời đọc {split.kind === "split" ? "phần đầu" : ""}<textarea value={split.leftText} onChange={e => setSplit({ ...split, leftText: e.target.value })} /></label>{split.kind === "split" && <><label>Lời gốc phần sau<textarea value={split.rightSource} onChange={e => setSplit({ ...split, rightSource: e.target.value })} /></label><label>Lời đọc phần sau<textarea value={split.rightText} onChange={e => setSplit({ ...split, rightText: e.target.value })} /></label></>}<button type="button" disabled={busy || !split.leftSource.trim() || (split.kind === "split" && !split.rightSource.trim())} onClick={() => void splitOrMerge()}>Lưu {split.kind === "split" ? "tách" : "gộp"}</button><button className="link" type="button" onClick={() => setSplit(null)}>Đóng</button></fieldset>}
          </div>}
        </>}
      </section>
    </div>
    <details className="draft-config"><summary>Dịch vụ, ngôn ngữ & loại kết quả</summary><div className="draft-config-grid">
      <label>Loại kết quả<select disabled={busy} value={config.audioOnly ? "audio" : config.textOnly ? "text" : "video"} onChange={e => configPatch({ audioOnly: e.target.value === "audio", textOnly: e.target.value === "text" })}><option value="video">Video lồng tiếng</option><option value="text">Video phụ đề</option><option value="audio">Audio</option></select></label>
      <label>Ngôn ngữ<select disabled={busy} value={config.target} onChange={e => configPatch({ target: e.target.value, voice: "" })}>{LANGUAGES.map(l => <option key={l.code} value={l.code}>{l.name}</option>)}</select></label>
      {!config.textOnly && <label>Giọng<select disabled={busy} value={config.voice} onChange={e => configPatch({ voice: e.target.value })}><option value="">Chọn giọng trong folder ngôn ngữ</option>{voices.map(v => <option key={v} value={v}>{v}</option>)}</select></label>}
      <label>Dịch bằng<select disabled={busy} value={config.provider} onChange={e => configPatch({ provider: e.target.value, model: "" })}>{Object.entries(MODEL_HINTS).map(([id, h]) => <option key={id} value={id}>{h.name}</option>)}</select></label>
      <label>Model<select disabled={busy} value={config.model} onChange={e => configPatch({ model: e.target.value })}><option value="">Mặc định ({hint?.fallback})</option>{config.model && !hint?.models.includes(config.model) && <option value={config.model}>{config.model}</option>}{hint?.models.map(m => <option key={m} value={m}>{m}</option>)}</select></label>
      <label className="draft-check"><input type="checkbox" checked={config.skipTranslate} disabled={busy} onChange={e => configPatch({ skipTranslate: e.target.checked })} />Không dịch</label><label className="draft-check"><input type="checkbox" checked={config.useTerms} disabled={busy} onChange={e => configPatch({ useTerms: e.target.checked })} />Duyệt thuật ngữ khi xuất</label>{!config.audioOnly && !config.textOnly && <label className="draft-check"><input type="checkbox" checked={config.burn} disabled={busy} onChange={e => configPatch({ burn: e.target.checked })} />Kèm phụ đề</label>}
      {online && <label>API key {hint?.name}<span className="draft-key"><input type="password" value={key} autoComplete="off" disabled={busy} onChange={e => setKey(e.target.value)} placeholder="Chỉ nhập khi cần lưu key mới" /><button type="button" disabled={busy || !key.trim()} onClick={() => void invoke("save_translation_choice", { provider: config.provider, apiKey: key.trim() }).then(() => { setKey(""); changed(); setNote("Đã lưu key trong cấu hình app."); }).catch(e => setError(errorText(e)))}>Lưu key</button></span></label>}
    </div>{config.provider === "deepseek-local" && !config.skipTranslate && <DeepseekServerRow server={deepseek} lockStop={busy || rangeBusy} after={changed} />}<button type="button" disabled={busy || !configDirty} onClick={() => void saveAll()}>Lưu cấu hình nháp</button></details>
    <StyleEditor audioOnly={config.audioOnly} value={config.render} onChange={render => configPatch({ render })} source={view.source} disabled={busy} captions={config.textOnly || config.burn} presets={[]} onSave={() => setNote("Lưu bộ cài đặt dùng mục Bộ cài đặt của app.")} onApply={() => {}} onDelete={() => {}} />
    {sample && <section className="draft-sample"><strong>Đoạn thử · căn từ phần đã chuẩn bị</strong><video src={sample} controls playsInline /><button className="link" type="button" onClick={() => setSample("")}>Đóng đoạn thử</button></section>}
    </div><footer className="draft-footer"><div><strong>{dirty ? "Có thay đổi chưa lưu" : "Nháp đã lưu"}</strong><small>Bản xuất trước là một phiên bản riêng.</small></div><div><button type="button" disabled={busy || !dirty} onClick={() => void saveAll()}>Lưu nháp</button><button type="button" className="link" disabled={busy || !!lock || config.audioOnly || time >= view.duration - 0.1} onClick={() => void sampleVideo()}>Dựng thử 8 giây</button>{view.lastExport && <button type="button" className="link" disabled={busy || rangeBusy} onClick={() => { pause(); onResult(view.lastExport!); }}>Mở bản xuất trước</button>}<button type="button" className="check" disabled={busy || !!lock} onClick={() => void exportVideo()}>{config.audioOnly ? "Xuất audio" : "Xuất video"}</button></div></footer>
  </div>;
}
