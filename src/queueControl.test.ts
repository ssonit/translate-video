import assert from "node:assert/strict";
import test from "node:test";
import { applyPresetToWaiting, moveWaiting, parseHistory } from "./queueControl.ts";
import { DEFAULT_RENDER, type ChannelPreset } from "./work/renderSettings.ts";
import type { QueueJob } from "./queue.ts";
const job=(id:string):QueueJob=>({id,label:id,mode:"file",source:`D:\\${id}.mp4`,folder:"D:\\out",target:"vi",voice:"voice-a",audioOnly:false,textOnly:false,burnCaptions:true,skipTranslate:false,translation:{provider:"deepseek-local",model:"deepseek-chat",useTerms:true},render:DEFAULT_RENDER});
test("reorder changes only waiting order and keeps job settings",()=>{
  const a=job("a"),b=job("b");const moved=moveWaiting([a,b],"b",-1);
  assert.deepEqual(moved,[b,a]);assert.equal(moved[0],b);
  assert.deepEqual(moveWaiting(moved,"b",-1),moved);
});
test("bulk preset affects selected jobs and preserves their translation service",()=>{
  const preset:ChannelPreset={id:"p",name:"Kênh",target:"en",voice:"en-voice",audioOnly:false,textOnly:false,burnCaptions:true,skipTranslate:false,useTerms:false,render:{...DEFAULT_RENDER,aspect:"9:16",audio:{original:50,voice:110,duck:true,duckLevel:20},pronunciations:{en:[{from:"ABC",to:"A B C"}]}}};
  const a=job("a"),b=job("b");const next=applyPresetToWaiting([a,b],["b"],preset);
  assert.equal(next[0],a);assert.equal(next[1].target,"en");assert.equal(next[1].translation?.provider,"deepseek-local");
  assert.equal(next[1].translation?.useTerms,false);assert.equal(next[1].render?.audio.original,50);
});
test("history restores failures without importing stray keys",()=>{
  const raw=JSON.stringify([{attempt:"one",state:"failed",job:{...job("a"),apiKey:"do not copy"},error:"Nguồn hỏng",elapsedMs:30,startedMs:1},{state:"bogus",job:job("b")}]);
  const list=parseHistory(raw);assert.equal(list.length,1);assert.equal(list[0].error,"Nguồn hỏng");assert.ok(!JSON.stringify(list).includes("apiKey"));
  assert.deepEqual(parseHistory("bad"),[]);
});
