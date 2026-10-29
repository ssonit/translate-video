import { useEffect, useRef, useState, type ReactNode, type CSSProperties } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { CAPTION_FONTS, DEFAULT_RENDER, type ChannelPreset, type RenderSettings } from "./renderSettings";
import "./StyleEditor.css";

export function StyleEditor({ value: s, onChange, source, disabled, captions, audioOnly, production, standalone = false, presets, onSave, onApply, onDelete }: {
  standalone?: boolean; production?: ReactNode; audioOnly: boolean; value: RenderSettings; onChange: (s: RenderSettings) => void; source: string; disabled: boolean; captions: boolean;
  presets: ChannelPreset[]; onSave: (name: string) => void; onApply: (p: ChannelPreset) => void; onDelete: (id: string) => void;
}) {
  const [file, setFile] = useState("");
  const [src, setSrc] = useState("");
  const [note, setNote] = useState("");
  const [name, setName] = useState("");
  const [guide, setGuide] = useState(true);
  const [markMask,setMarkMask]=useState(false);
  const [logoSrc,setLogoSrc]=useState("");
  const [ratio, setRatio] = useState(16/9);
  const [width, setWidth] = useState(320);
  const [sample, setSample] = useState("Một câu chuyện mới bắt đầu từ đây.");
  const box = useRef<HTMLDivElement>(null);
  const video = useRef<HTMLVideoElement>(null);
  const backdrop = useRef<HTMLVideoElement>(null);
  const drag = useRef<{ kind: "caption" | "frame" | "mask"; sx: number; sy: number; x: number; y: number; focusX: number; focusY: number } | null>(null);
  const path = source || file;
  useEffect(() => {
    let current = true;
    setSrc(""); setNote("");
    if (path) invoke("allow_video_preview", { path }).then(() => { if (current) setSrc(convertFileSrc(path)); })
      .catch((e) => current && setNote(String(e)));
    return () => { current = false; };
  }, [path]);
  useEffect(() => {
    if (!box.current) return;
    const observer = new ResizeObserver(([e]) => setWidth(e.contentRect.width));
    observer.observe(box.current);
    return () => observer.disconnect();
  }, [audioOnly]);
  useEffect(()=>{
    let live=true;setLogoSrc("");
    if(s.logo.enabled)invoke("allow_logo_preview",{path:s.logo.path,sha256:s.logo.sha256}).then(()=>{if(live)setLogoSrc(convertFileSrc(s.logo.path));}).catch(e=>live&&setNote(String(e)));
    return ()=>{live=false;};
  },[s.logo.enabled,s.logo.path,s.logo.sha256]);
  const update = (patch: Partial<RenderSettings>) => onChange({ ...s, ...patch });
  const outputRatio = s.aspect === "original" ? ratio : s.aspect === "9:16" ? 9/16 : s.aspect === "16:9" ? 16/9 : 1;
  const boxHeight=width/outputRatio;
  const cover=s.aspect==='original'||s.framing==='crop';
  const imageWidth=cover?Math.max(width,boxHeight*ratio):Math.min(width,boxHeight*ratio);
  const imageHeight=imageWidth/ratio;
  const imageLeft=(width-imageWidth)*(cover?s.focusX/100:0.5);
  const imageTop=(boxHeight-imageHeight)*(cover?s.focusY/100:0.5);
  const sourcePoint=(cx:number,cy:number,r:DOMRect)=>({x:Math.max(0,Math.min(100,(cx-r.left-imageLeft)/imageWidth*100)),y:Math.max(0,Math.min(100,(cy-r.top-imageTop)/imageHeight*100))});
  const fontSize = Math.min(width, width / outputRatio) * s.size / 100;
  const range = (label: string, key: "size" | "outline" | "focusX" | "focusY" | "captionX" | "captionY", min: number, max: number, step = 1) => (
    <label>{label}<span className="style-range"><input type="range" min={min} max={max} step={step} value={s[key]} onChange={(e) => update({ [key]: +e.target.value })} /><output>{s[key]}{key === "outline" ? "" : "%"}</output></span></label>
  );
  async function pick() {
    try {
      const selected = await open({ multiple: false, filters: [{ name: "Video", extensions: ["mp4","mov","mkv","webm","avi","m4v"] }] });
      if (typeof selected === "string") setFile(selected);
    } catch (e) { setNote(String(e)); }
  }
  return <details className={standalone ? "style-editor is-standalone" : "style-editor"} open={standalone ? true : undefined}>
    <summary><span><strong>{audioOnly ? "Bộ cài đặt cho audio" : "Khung hình & kiểu chữ"}</strong><small>{s.aspect === "original" ? "Giữ khung gốc" : s.aspect} · {s.font} · {presets.length} bộ cài đặt</small></span><span aria-hidden="true">⌄</span></summary>
    <div className="style-body">
      {!standalone ? <div className="style-presets">
        <label>Bộ cài đặt đã lưu<select disabled={disabled} value="" onChange={(e) => { const p = presets.find((p) => p.id === e.target.value); if (p) onApply(p); }}><option value="">Chọn để áp dụng…</option>{presets.map((p) => <option key={p.id} value={p.id}>{p.name}</option>)}</select></label>
        <form onSubmit={(e) => { e.preventDefault(); if (name.trim()) { onSave(name.trim()); setName(""); } }}><input aria-label="Tên bộ cài đặt" placeholder="Tên kênh hoặc phong cách" maxLength={60} value={name} onChange={(e) => setName(e.target.value)} disabled={disabled} /><button className="link" disabled={disabled || !name.trim()}>Lưu bộ mới</button></form>
        {presets.length > 0 && <details><summary>Quản lý bộ đã lưu</summary>{presets.map((p) => <div className="style-preset-row" key={p.id}><span>{p.name}</span><button type="button" className="link" disabled={disabled} onClick={() => onDelete(p.id)}>Xóa</button></div>)}</details>}
        <p className="key-note">Lưu khung, chữ, ngôn ngữ, giọng và loại kết quả hiện tại trên máy.</p>
      </div> : null}
      {production}
      {audioOnly ? <p className="key-note">Khung hình và kiểu chữ được giữ lại cho lần xuất video. Audio chỉ dùng ngôn ngữ và giọng đã chọn.</p> : <div className="style-layout">
        <div className="style-preview-column">
          <div className="style-preview" ref={box} style={{ aspectRatio: outputRatio }} onPointerDown={(e) => {
            if (disabled) return;
            const kind = markMask && s.mask.enabled ? "mask" : captions && (e.target as HTMLElement).closest(".style-caption") ? "caption" : src && s.aspect !== "original" && s.framing === "crop" ? "frame" : null;
            if (!kind) return;
            const point=sourcePoint(e.clientX,e.clientY,e.currentTarget.getBoundingClientRect());
            drag.current = { kind, sx:point.x, sy:point.y, x: e.clientX, y: e.clientY, focusX: s.focusX, focusY: s.focusY };
            e.currentTarget.setPointerCapture(e.pointerId);
          }} onPointerMove={(e) => {
            if (!e.currentTarget.hasPointerCapture(e.pointerId)) return;
            const r = e.currentTarget.getBoundingClientRect();
            const d = drag.current;
            if(d?.kind === "mask"){
              const point=sourcePoint(e.clientX,e.clientY,r);
              const x=Math.min(99,Math.min(point.x,d.sx)),y=Math.min(99,Math.min(point.y,d.sy));
              update({mask:{...s.mask,x,y,width:Math.min(100-x,Math.max(1,Math.abs(point.x-d.sx))),height:Math.min(100-y,Math.max(1,Math.abs(point.y-d.sy)))}});
            }else if (d?.kind === "frame") {
              const overflowX = Math.max(0, r.height * ratio - r.width);
              const overflowY = Math.max(0, r.width / ratio - r.height);
              update({ focusX: overflowX > 0.5 ? Math.round(Math.max(0,Math.min(100,d.focusX-(e.clientX-d.x)/overflowX*100))) : s.focusX,
                focusY: overflowY > 0.5 ? Math.round(Math.max(0,Math.min(100,d.focusY-(e.clientY-d.y)/overflowY*100))) : s.focusY });
            } else {
              update({ captionX: Math.round(Math.max(15,Math.min(85,(e.clientX-r.left)/r.width*100))), captionY: Math.round(Math.max(20,Math.min(92,(e.clientY-r.top)/r.height*100))) });
            }
          }} onPointerUp={(e) => { if (e.currentTarget.hasPointerCapture(e.pointerId)) e.currentTarget.releasePointerCapture(e.pointerId); }}>
            {src && s.framing === "blur" && s.aspect !== "original" && <video ref={backdrop} className="style-backdrop" src={src} muted playsInline tabIndex={-1} aria-hidden="true" />}
            {src ? <video ref={video} className="style-video" src={src} muted playsInline preload="metadata" style={{ objectFit: s.aspect === "original" || s.framing === "crop" ? "cover" : "contain", objectPosition: cover ? `${s.focusX}% ${s.focusY}%` : "50% 50%" }}
              onLoadedMetadata={(e) => setRatio(e.currentTarget.videoWidth / e.currentTarget.videoHeight || 16/9)}
              onError={() => setNote("Không phát được định dạng này trong cửa sổ. Bạn vẫn có thể xuất bằng FFmpeg.")} /> : <div className="style-empty">Chọn video để thử bố cục<br /><small>Hoặc chỉnh chữ trên khung mẫu</small></div>}
            {s.mask.enabled&&src&&<div className="source-mask-preview" style={{left:imageLeft+imageWidth*s.mask.x/100,top:imageTop+imageHeight*s.mask.y/100,width:imageWidth*s.mask.width/100,height:imageHeight*s.mask.height/100,background:s.mask.mode==='solid'?s.mask.color:'transparent',backdropFilter:s.mask.mode==='blur'?'blur(8px)':undefined,outline:markMask?'1px dashed var(--accent)':'none'}}/>}
            {s.logo.enabled&&logoSrc&&<img className="style-logo" src={logoSrc} alt="Logo kênh" style={{width:`${s.logo.size}%`,opacity:s.logo.opacity/100,left:`${s.logo.x}%`,top:`${s.logo.y}%`,transform:`translate(-${s.logo.x}%,-${s.logo.y}%)`}}/>}
            {guide && <div className="style-safe"><span>Vùng gợi ý đặt chữ</span></div>}
            {captions && <div className="style-caption" style={{ left: `${s.captionX}%`, top: `${s.captionY}%`, fontFamily: s.font, fontSize, color: s.color,
              maxWidth: `${2 * (Math.min(s.captionX,100-s.captionX)-7)}%`,
              WebkitTextStroke: `${s.outline * fontSize / 45.36}px ${s.outlineColor}`, background: s.background ? s.outlineColor : "transparent" } as CSSProperties}>{sample}</div>}
          </div>
          {src && <label className="style-seek">Tìm khung xem thử<input type="range" min={0} max={100} defaultValue={0} onChange={(e) => {
            if (!video.current || !Number.isFinite(video.current.duration)) return;
            const time = video.current.duration * +e.target.value / 100;
            video.current.currentTime = Math.min(time, Math.max(0,video.current.duration-0.05));
            if (backdrop.current) backdrop.current.currentTime = video.current.currentTime;
          }} /></label>}
          {!source && <button type="button" className="link" onClick={() => void pick()}>Chọn video xem thử</button>}
          {s.mask.enabled&&<label className="style-check"><input type="checkbox" checked={markMask} disabled={disabled} onChange={e=>setMarkMask(e.target.checked)}/>Khoanh vùng chữ nguồn trên hình</label>}
          <label className="style-check"><input type="checkbox" checked={guide} onChange={(e) => setGuide(e.target.checked)} />Hiện vùng gợi ý tránh giao diện video ngắn</label>
          <p className="key-note">{captions ? "Kéo chữ để đổi vị trí; kéo hình khi chọn Cắt đầy khung. " : "Bật Kèm phụ đề hoặc chọn Video phụ đề để chỉnh chữ. "}Đây là mẫu bố cục; bản xuất dùng font và cách xuống dòng của FFmpeg. Vùng gợi ý không ghi vào video.</p>
          {note && <p className="error" role="status">{note}</p>}
        </div>
        <fieldset className="style-controls" disabled={disabled}>
          <label>Khung xuất<select value={s.aspect} onChange={(e) => update({ aspect: e.target.value as RenderSettings["aspect"] })}><option value="original">Giữ khung gốc</option><option value="9:16">9:16 · Dọc</option><option value="1:1">1:1 · Vuông</option><option value="16:9">16:9 · Ngang</option></select></label>
          {s.aspect !== "original" && <><label>Cách đặt hình<select value={s.framing} onChange={(e) => update({ framing: e.target.value as RenderSettings["framing"] })}><option value="blur">Giữ toàn hình · nền mờ</option><option value="crop">Cắt đầy khung</option></select></label>{s.framing === "crop" && <>{range("Giữ nhân vật theo chiều ngang","focusX",0,100)}{range("Giữ nhân vật theo chiều dọc","focusY",0,100)}</>}</>}
          {captions && <><label>Chữ mẫu<input value={sample} maxLength={160} onChange={(e) => setSample(e.target.value)} /></label><label>Font<select value={s.font} onChange={(e) => update({ font: e.target.value })}>{CAPTION_FONTS.map((f) => <option key={f}>{f}</option>)}</select></label>
            {range("Cỡ chữ theo cạnh ngắn","size",2,8,0.1)}
            <div className="style-colors"><label>Màu chữ<input type="color" value={s.color} onChange={(e) => update({ color: e.target.value })} /></label><label>Màu viền / nền<input type="color" value={s.outlineColor} onChange={(e) => update({ outlineColor: e.target.value })} /></label></div>
            {range("Độ dày viền","outline",0,5,0.1)}<label className="style-check"><input type="checkbox" checked={s.background} onChange={(e) => update({ background: e.target.checked })} />Nền sau chữ</label>
            {range("Vị trí chữ ngang","captionX",15,85)}{range("Vị trí chữ dọc","captionY",20,92)}</>}
          <button type="button" className="link" onClick={() => onChange({ ...DEFAULT_RENDER })}>Về bố cục mặc định</button>
        </fieldset>
      </div>}
    </div>
  </details>;
}
