import { useEffect, useState } from "react";
import type { QueueJob } from "../queue";
import type { QueuePolicy, QueueRecord } from "../queueControl";
import type { ChannelPreset } from "./renderSettings";
import { duration, languageName } from "./format";
import { MODEL_HINTS } from "./models";

export function QueuePanel({queue,history,policy,presets,running,needsKey,onPolicy,onMove,onDrop,onUpdate,onBulk,onRetry,onOpen}: {
  queue:QueueJob[];history:QueueRecord[];policy:QueuePolicy;presets:ChannelPreset[];running:boolean;needsKey:boolean;
  onPolicy:(p:QueuePolicy)=>void;onMove:(id:string,delta:number)=>void;onDrop:(id:string)=>void;onUpdate:(id:string)=>void;
  onBulk:(ids:string[],preset:ChannelPreset)=>void;onRetry:(record:QueueRecord)=>void;onOpen:(path:string)=>void;
}) {
  const [selected,setSelected]=useState<string[]>([]);
  const [presetId,setPresetId]=useState("");
  useEffect(()=>setSelected(ids=>ids.filter(id=>queue.some(j=>j.id===id))),[queue]);
  const preset=presets.find(p=>p.id===presetId);
  return <div className="queue-panel">
    <details className="queue-policy"><summary>Hàng đợi · {queue.length} video chờ · {policy==="stop"?"Dừng khi lỗi":"Tiếp tục khi lỗi"}</summary>
      <label>Khi một video lỗi<select disabled={running} value={policy} onChange={e=>onPolicy(e.target.value as QueuePolicy)}><option value="stop">Dừng để kiểm tra</option><option value="continue">Lưu lỗi, chạy video sau</option></select></label>
      <p className="key-note">Video chờ kiểm tra tên vẫn dừng để bạn duyệt. Việc dở được giữ lại khi đóng app; mở lại cần bấm Chạy.</p>
    </details>
    {queue.length>0&&<div className="wait-list">
      <div className="queue-bulk"><label><input type="checkbox" checked={selected.length===queue.length} onChange={e=>setSelected(e.target.checked?queue.map(j=>j.id):[])}/>Chọn tất cả ({queue.length})</label>
        {selected.length>0&&<><select aria-label="Bộ cài đặt cho các video đã chọn" value={presetId} onChange={e=>setPresetId(e.target.value)}><option value="">Chọn bộ cài đặt…</option>{presets.map(p=><option value={p.id} key={p.id}>{p.name}</option>)}</select>
          {preset&&<p className="key-note">Sẽ đổi {selected.length} video: {languageName(preset.target)}, {preset.voice||"không giọng"}, {preset.audioOnly?"audio":preset.textOnly?"phụ đề":"lồng tiếng"}, {preset.render.aspect}. Giữ dịch vụ dịch của từng video.</p>}
          <button type="button" className="link" disabled={!preset} onClick={()=>{if(preset)onBulk(selected,preset);}}>Áp dụng cho {selected.length} video</button></>}
      </div>
      {queue.map((j,i)=><div className="wait-row" key={j.id}>
        <input type="checkbox" aria-label={`Chọn ${j.label}`} checked={selected.includes(j.id)} onChange={e=>setSelected(ids=>e.target.checked?[...ids,j.id]:ids.filter(id=>id!==j.id))}/>
        <span title={j.source}><strong>{j.label}</strong><em>{languageName(j.target)} · {j.audioOnly?"Audio":j.textOnly?"Phụ đề":"Lồng tiếng"}{!j.audioOnly&&j.render?` · ${j.render.aspect}`:""}{j.voice&&!j.textOnly?` · ${j.voice}`:""}</em>
          <em>{j.skipTranslate?"Không dịch":j.translation?`${MODEL_HINTS[j.translation.provider]?.name??j.translation.provider} · ${j.translation.model}`:"Dùng dịch vụ đang chọn khi bắt đầu"}</em></span>
        <div className="queue-row-actions"><button type="button" className="link" aria-label={`Đưa ${j.label} lên`} disabled={i===0} onClick={()=>onMove(j.id,-1)}>↑</button><button type="button" className="link" aria-label={`Đưa ${j.label} xuống`} disabled={i===queue.length-1} onClick={()=>onMove(j.id,1)}>↓</button>
          <button type="button" className="link" disabled={needsKey} onClick={()=>onUpdate(j.id)}>Dùng lựa chọn hiện tại</button><button type="button" className="link" onClick={()=>onDrop(j.id)}>Bỏ</button></div>
      </div>)}
    </div>}
    {history.length>0&&<details className="queue-history"><summary>Các lượt đã chạy · {history.filter(r=>r.state==="failed").length} lượt lỗi</summary>
      {[...history].reverse().slice(0,12).map(r=><article key={r.attempt}><strong>{r.job.label}</strong><p>{r.state==="done"?"Đã xong":r.state==="failed"?"Lỗi":"Đã bỏ qua"} · {duration(r.elapsedMs/1000)} · {new Date(r.startedMs).toLocaleString("vi-VN")}</p>
        {r.error&&<p className="error">{r.error}</p>}{r.result&&<button type="button" className="link" onClick={()=>onOpen(r.result)}>Mở kết quả</button>}{r.state!=="done"&&<button type="button" className="link" onClick={()=>onRetry(r)}>Xếp lại lượt này</button>}</article>)}
    </details>}
  </div>;
}
