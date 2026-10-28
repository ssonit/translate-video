import { parseSavedQueue, type QueueJob } from "./queue.ts";
import type { ChannelPreset } from "./work/renderSettings.ts";

export type QueuePolicy = "stop" | "continue";
export type QueueRecord = { attempt: string; job: QueueJob; state: "done" | "failed" | "skipped"; result: string; error: string; startedMs: number; elapsedMs: number };
export function moveWaiting(queue: QueueJob[],id:string,delta:number): QueueJob[] {
  const index=queue.findIndex(j=>j.id===id), target=index+delta;
  if(index<0||target<0||target>=queue.length)return queue;
  const next=[...queue];[next[index],next[target]]=[next[target],next[index]];return next;
}
export function applyPresetToWaiting(queue:QueueJob[],ids:string[],preset:ChannelPreset):QueueJob[]{
  const picked=new Set(ids);
  return queue.map(j=>picked.has(j.id)?{...j,target:preset.target,voice:preset.voice,audioOnly:preset.audioOnly,textOnly:preset.textOnly,burnCaptions:preset.burnCaptions,skipTranslate:preset.skipTranslate,render:preset.render,
    translation:j.translation?{...j.translation,useTerms:preset.useTerms}:undefined}:j);
}
export function parseHistory(raw:string|null):QueueRecord[]{
  try {const list:unknown=JSON.parse(raw??"[]");if(!Array.isArray(list))return [];
    return list.flatMap(r=>{
      if(!r||!["done","failed","skipped"].includes(r.state)||typeof r.attempt!=="string")return [];
      const job=parseSavedQueue(JSON.stringify({waiting:[r.job]})).waiting[0];
      if(!job)return [];
      return [{attempt:r.attempt,job,state:r.state,result:typeof r.result==="string"?r.result:"",error:typeof r.error==="string"?r.error.slice(0,1500):"",startedMs:typeof r.startedMs==="number"&&Number.isFinite(r.startedMs)?r.startedMs:0,elapsedMs:typeof r.elapsedMs==="number"&&Number.isFinite(r.elapsedMs)?Math.max(0,r.elapsedMs):0} as QueueRecord];
    }).slice(-50);
  } catch{return [];}
}
export function readQueuePolicy():QueuePolicy {try{return localStorage.getItem("queue-error-policy")==="continue"?"continue":"stop";}catch{return "stop";}}
export function readQueueHistory():QueueRecord[]{try{return parseHistory(localStorage.getItem("queue-history"));}catch{return [];}}
