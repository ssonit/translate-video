import { useEffect, useRef, useState } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { Logo, Pronunciation, RenderSettings } from "./renderSettings";
import { playVoicePreview } from "../tts/voicePreview";

export function ProductionOptions({section,onChange,value:s,lang,voice,source,folder,audioOnly,textOnly,burn,skipTranslate,useTerms,disabled,onBusy}: {
  section?: "visual" | "audio" | "sample"; value:RenderSettings;onChange:(value:RenderSettings)=>void;lang:string;voice:string;source:string;folder:string;
  audioOnly:boolean;textOnly:boolean;burn:boolean;skipTranslate:boolean;useTerms:boolean;disabled:boolean;onBusy:(busy:boolean)=>void;
}) {
  const [note,setNote]=useState("");
  const [sample,setSample]=useState("");
  const [start,setStart]=useState(0);
  const [busy,setBusy]=useState(false);
  const [from,setFrom]=useState("");
  const [to,setTo]=useState("");
  const [playing,setPlaying]=useState(false);
  const voiceAbort=useRef<AbortController|null>(null);
  const [stopping,setStopping]=useState(false);
  const active=useRef(false);
  const rules=s.pronunciations[lang]??[];
  const update=(patch:Partial<RenderSettings>)=>onChange({...s,...patch});
  useEffect(()=>{setSample("");},[s,source,lang,textOnly,burn]);
  useEffect(()=>()=>{voiceAbort.current?.abort();if(active.current)void invoke("cancel_dub");},[]);
  const range=(label:string,value:number,max:number,onValue:(v:number)=>void)=><label>{label}<span className="style-range"><input type="range" min={0} max={max} value={value} onChange={e=>onValue(+e.target.value)}/><output>{value}%</output></span></label>;
  async function chooseLogo(){
    setNote("");
    try {const path=await open({multiple:false,filters:[{name:"Ảnh logo",extensions:["png","jpg","jpeg"]}]});if(typeof path!=="string")return;
      const logo=await invoke<Logo>("import_logo",{path});update({logo});
    } catch(e){setNote(String(e));}
  }
  function saveRules(next:Pronunciation[]){update({pronunciations:{...s.pronunciations,[lang]:next}});}
  async function hear(text:string){
    if(voiceAbort.current)return;
    const controller=new AbortController();voiceAbort.current=controller;
    setPlaying(true);setNote("");onBusy(true);
    try {await playVoicePreview(lang,voice,controller.signal,text);if(controller.signal.aborted)setNote("Đã dừng thử giọng.");}
    catch(e){setNote(String(e));}finally{voiceAbort.current=null;setPlaying(false);setStopping(false);onBusy(false);}
  }
  async function cancelWork(){
    setStopping(true);setNote("Đang dừng…");
    if(voiceAbort.current){voiceAbort.current.abort();return;}
    try{await invoke("cancel_dub");}catch(e){setNote(String(e));setStopping(false);}
  }
  async function preview(){
    setSample("");setNote("");setBusy(true);onBusy(true);active.current=true;
    try {const path=await invoke<string>("render_sample",{video:source,outputDir:folder,target:lang,start,seconds:8,textOnly,burn,voice,skipTranslate,useTerms,render:s});setSample(convertFileSrc(path));}
    catch(e){setNote(String(e));}finally{active.current=false;setBusy(false);setStopping(false);onBusy(false);}
  }
  return <div className="production-options">
    {playing&&<button type="button" className="link" disabled={stopping} onClick={()=>void cancelWork()}>{stopping?"Đang dừng…":"Dừng thử giọng"}</button>}
    {section === "sample" && audioOnly ? <p className="key-note">Đoạn thử hình chỉ dùng cho video. Dùng Nghe trước trên màn Lồng tiếng để nghe audio.</p> : null}
    {section === "audio" && textOnly ? <p className="key-note">Video phụ đề giữ tiếng gốc, không dùng giọng đọc mới.</p> : null}
    {section === "audio" && audioOnly ? <p className="key-note">Audio dùng giọng đã chọn và các cách đọc tên bên dưới.</p> : null}
    {(!section || section === "audio")&&!audioOnly&&!textOnly&&<details open={section ? true : undefined}><summary>Âm thanh · gốc {s.audio.original}% · lời Việt {s.audio.voice}%</summary>
      <fieldset className="style-controls" disabled={disabled||busy||playing}>
        {range("Tiếng gốc",s.audio.original,100,v=>update({audio:{...s.audio,original:v}}))}
        {range("Tiếng lồng",s.audio.voice,200,v=>update({audio:{...s.audio,voice:v}}))}
        <label className="style-check"><input type="checkbox" checked={s.audio.duck} onChange={e=>update({audio:{...s.audio,duck:e.target.checked}})}/>Tự hạ tiếng gốc khi có lời Việt</label>
        {s.audio.duck&&range("Mức còn lại trong lúc có lời",s.audio.duckLevel,100,v=>update({audio:{...s.audio,duckLevel:v}}))}
        <p className="key-note">Hạ cả lời gốc và nhạc đang chung một track. Khoảng nghỉ dùng lại mức tiếng gốc đã chọn.</p>
      </fieldset>
    </details>}
    {(!section || section === "audio")&&!textOnly&&<details open={section ? true : undefined}><summary>Cách đọc tên · {rules.length} mục cho {lang}</summary>
      <p className="key-note">Chỉ đổi chữ đưa vào giọng đọc. Tên trên phụ đề và bản dịch giữ nguyên. Lưu bộ cài đặt để dùng lại cho kênh.</p>
      <fieldset className="style-controls" disabled={disabled||busy||playing}>
        {rules.map((r,i)=><div className="pronunciation-row" key={i}><span><strong>{r.from}</strong><small>Đọc: {r.to}</small></span><button type="button" className="link" disabled={!voice} onClick={()=>void hear(r.to)}>Nghe</button><button type="button" className="link" aria-label={`Bỏ cách đọc ${r.from}`} onClick={()=>saveRules(rules.filter((_,j)=>j!==i))}>Bỏ</button></div>)}
        <label>Tên trên phụ đề<input value={from} maxLength={70} onChange={e=>setFrom(e.target.value)} placeholder="Ví dụ: Toronto"/></label>
        <label>Chữ dùng để đọc<input value={to} maxLength={140} onChange={e=>setTo(e.target.value)} placeholder="Nhập cách đọc bạn muốn"/></label>
        <div className="production-actions"><button type="button" className="link" disabled={!from.trim()||!to.trim()||rules.length>=200} onClick={()=>{saveRules([...rules.filter(r=>r.from.toLocaleLowerCase()!==from.trim().toLocaleLowerCase()),{from:from.trim(),to:to.trim()}]);setFrom("");setTo("");}}>Thêm / thay cách đọc</button><button type="button" className="link" disabled={!to.trim()||!voice} onClick={()=>void hear(to)}>Nghe thử</button></div>
      </fieldset>
    </details>}
    {(!section || section === "visual")&&!audioOnly&&<details open={section ? true : undefined}><summary>Logo kênh {s.logo.enabled?"· Đang bật":""}</summary>
      <fieldset className="style-controls" disabled={disabled||busy}>
        <button type="button" className="link" onClick={()=>void chooseLogo()}>Chọn ảnh PNG / JPEG</button>
        {s.logo.path&&<><label className="style-check"><input type="checkbox" checked={s.logo.enabled} onChange={e=>update({logo:{...s.logo,enabled:e.target.checked}})}/>Đặt logo lên video</label>
          {range("Kích thước theo chiều ngang",s.logo.size,50,v=>update({logo:{...s.logo,size:Math.max(3,v)}}))}
          {range("Độ rõ",s.logo.opacity,100,v=>update({logo:{...s.logo,opacity:v}}))}
          {range("Vị trí ngang",s.logo.x,100,v=>update({logo:{...s.logo,x:v}}))}
          {range("Vị trí dọc",s.logo.y,100,v=>update({logo:{...s.logo,y:v}}))}</>}
      </fieldset>
    </details>}
    {(!section || section === "visual")&&!audioOnly&&<details open={section ? true : undefined}><summary>Che / làm mờ chữ nguồn {s.mask.enabled?"· Đang bật":""}</summary>
      <fieldset className="style-controls" disabled={disabled||busy}>
        <label className="style-check"><input type="checkbox" checked={s.mask.enabled} onChange={e=>update({mask:{...s.mask,enabled:e.target.checked}})}/>Xử lý vùng chữ gốc</label>
        {s.mask.enabled&&<><label>Cách xử lý<select value={s.mask.mode} onChange={e=>update({mask:{...s.mask,mode:e.target.value as "blur"|"solid"}})}><option value="blur">Làm mờ</option><option value="solid">Che bằng màu nền</option></select></label>
          <div className="mask-fields">{([['x','Trái'],['y','Trên'],['width','Rộng'],['height','Cao']] as const).map(([key,label])=><label key={key}>{label} (%)<input type="number" min={key==='width'||key==='height'?1:0} max={100} value={s.mask[key]} onChange={e=>{
            const mask={...s.mask,[key]:+e.target.value};mask.x=Math.max(0,Math.min(99,mask.x));mask.y=Math.max(0,Math.min(99,mask.y));mask.width=Math.max(1,Math.min(100-mask.x,mask.width));mask.height=Math.max(1,Math.min(100-mask.y,mask.height));update({mask});
          }}/></label>)}</div>
          {s.mask.mode==='solid'&&<label>Màu che<input type="color" value={s.mask.color} onChange={e=>update({mask:{...s.mask,color:e.target.value}})}/></label>}
          <p className="key-note">Bật “Khoanh vùng chữ nguồn” trong khung mẫu để kéo vùng trên hình. Vùng cố định suốt video; đây là che/làm mờ, không phục hồi cảnh phía sau.</p></>}
      </fieldset>
    </details>}
    {(!section || section === "sample")&&!audioOnly&&<details open={section ? true : undefined}><summary>Xem thử bản xuất · 8 giây</summary>
      <label>Từ giây<input type="number" min={0} step={1} value={start} disabled={busy} onChange={e=>{setSample("");setStart(Math.max(0,+e.target.value));}}/></label>
      <p className="key-note">Dùng khung, chữ và mức âm khi xuất. Cần có bản dịch và đủ WAV từng câu; chưa có thì chạy video trước.</p>
      <div className="production-actions"><button type="button" className="link" disabled={disabled||busy||!source||!folder} onClick={()=>void preview()}>{busy?"Đang dựng đoạn thử…":"Dựng & xem thử"}</button>{busy&&<button type="button" className="link" disabled={stopping} onClick={()=>void cancelWork()}>{stopping?"Đang dừng…":"Dừng đoạn thử"}</button>}</div>
      {sample&&<video className="export-sample" src={sample} controls preload="metadata"/>}
    </details>}
    {note&&<p className="error" role="alert">{note}</p>}
  </div>;
}
