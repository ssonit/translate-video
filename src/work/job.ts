import { applyPresetToWaiting, moveWaiting, type QueuePolicy, type QueueRecord } from "../queueControl";
import { cleanRender, DEFAULT_RENDER, type ChannelPreset, type RenderSettings } from "./renderSettings";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import {
  appendJobs,
  queueKey,
  queueLabel,
  takeActive,
  writeSavedQueue,
  type QueueJob,
} from "../queue";
import type { WatchUpdate } from "../WatchLive";
import { errorText, fileName, freshSteps } from "./format";
import type { Session } from "./state";
import type { FinishedDub, JobSpec, Prefs } from "./types";
import { linkProblem } from "./linkCheck";
import { MODEL_HINTS } from "./models";

export function jobActions(s: Session) {
  function chooseQueuePolicy(policy:QueuePolicy){
    s.setQueuePolicy(policy);
    try{localStorage.setItem("queue-error-policy",policy);}catch{s.setQueueNote("Chỉ lưu chính sách lỗi trong phiên này.");}
  }
  function recordRun(spec:JobSpec,state:QueueRecord["state"],startedMs:number,result="",error=""){
    const job=s.activeJob.current??{...spec,id:crypto.randomUUID(),label:queueLabel(spec.mode,spec.source)};
    const pause=s.runPauseRef.current;
    const elapsedMs=Math.max(0,Date.now()-startedMs-pause.pausedMs-(pause.sinceMs===null?0:Date.now()-pause.sinceMs));
    const record:QueueRecord={attempt:crypto.randomUUID(),job,state,result,error,startedMs,elapsedMs};
    const next=[...s.queueHistoryRef.current,record].slice(-50);
    s.queueHistoryRef.current=next;s.setQueueHistory(next);
    try{localStorage.setItem("queue-history",JSON.stringify(next));}catch{s.setQueueNote("Không lưu được lịch sử; kết quả vẫn ở folder đã chọn.");}
  }
  function bulkPreset(ids:string[],preset:ChannelPreset){const waiting=s.queueRef.current.map(j=>ids.includes(j.id)&&!j.translation?{...j,translation:translationSnapshot()}:j);rememberQueue(applyPresetToWaiting(waiting,ids,preset));s.setQueueNote(`Đã áp dụng ${preset.name} cho các video đang chờ được chọn.`);}
  async function retryRecord(record:QueueRecord){const added=pushJobs([{...record.job,id:crypto.randomUUID()}]);if(added>0&&!s.running)await runQueue();}
  async function skipCurrent(){s.skipActive.current=true;s.holdQueue.current=false;await invoke("cancel_dub");}

  function chooseRender(value: RenderSettings) {
    const next = cleanRender(value);
    s.setRender(next);
    try { localStorage.setItem("video-render", JSON.stringify(next)); s.setPrefsNote(""); }
    catch { s.setPrefsNote("Không lưu được bố cục trên máy. Lựa chọn vẫn dùng trong phiên này."); }
  }

  function storePresets(next: ChannelPreset[]) {
    try { localStorage.setItem("channel-presets", JSON.stringify(next)); s.setPresets(next); s.setPrefsNote(""); }
    catch { s.setPrefsNote("Không lưu được bộ cài đặt trên máy."); }
  }

  function savePreset(name: string) {
    storePresets([...s.presets, { id: crypto.randomUUID(), name: name.trim().slice(0,60), render: s.render,
      target: s.target, voice: s.voice, audioOnly: s.audioOnly, textOnly: s.textOnly,
      burnCaptions: s.burnCaptions, skipTranslate: s.skipTranslate, useTerms: s.useTerms }]);
  }

  async function applyPreset(p: ChannelPreset) {
    if (s.running || s.captioning || s.gate.current) return;
    s.gate.current = true;
    s.setPresetApplying(true);
    s.setPrefsNote("");
    try {
      // Save before switching language so its voice-loading effect sees this choice.
      if (p.voice && !p.textOnly) await invoke("save_voice", { lang: p.target, name: p.voice });
      const prefs = await invoke<Prefs>("save_prefs", { useTerms: p.useTerms, skipTranslate: p.skipTranslate, burnCaptions: p.burnCaptions });
      s.setUseTerms(prefs.useTerms); s.setSkipTranslate(prefs.skipTranslate); s.setBurnCaptions(prefs.burnCaptions);
      s.setTarget(p.target); s.setVoice(p.voice); s.setAudioOnly(p.audioOnly); s.setTextOnly(p.textOnly);
      localStorage.setItem("target-language", p.target);
      chooseRender(p.render);
    } catch (e) { s.setPrefsNote(errorText(e)); }
    finally { s.gate.current = false; s.setPresetApplying(false); }
  }

  function rememberQueue(next: QueueJob[]) {
    s.queueRef.current = next;
    s.setQueue(next);
    writeSavedQueue({ waiting: next, current: s.activeJob.current });
  }

  function noteRunning(spec: JobSpec, label = "", id = "") {
    // Chạy lại sau khi đã tải xong thì nguồn đổi từ link sang file, nhưng vẫn là cùng một việc.
    const same =
      s.activeJob.current &&
      ((id !== "" && s.activeJob.current.id === id) ||
        queueKey(s.activeJob.current.mode, s.activeJob.current.source) === queueKey(spec.mode, spec.source));
    const job: QueueJob = {
      id: id || (same ? s.activeJob.current!.id : "") || `live-${Date.now().toString(36)}`,
      label: label || (same ? s.activeJob.current!.label : "") || queueLabel(spec.mode, spec.source),
      mode: spec.mode,
      source: spec.source,
      folder: spec.folder,
      target: spec.target,
      voice: spec.voice,
      audioOnly: spec.audioOnly,
      skipTranslate: spec.skipTranslate,
      burnCaptions: spec.burnCaptions,
      textOnly: spec.textOnly,
      translation: spec.translation,
      render: spec.render,
    };
    if (same) {
      s.activeJob.current = job;
      writeSavedQueue({ waiting: s.queueRef.current, current: job });
      return;
    }
    const taken = takeActive(s.queueRef.current, s.activeJob.current, job);
    s.activeJob.current = taken.active;
    rememberQueue(taken.waiting);
  }

  function releaseRunning(spec: JobSpec) {
    const active = s.activeJob.current;
    if (!active || queueKey(active.mode, active.source) !== queueKey(spec.mode, spec.source)) return;
    s.activeJob.current = null;
    writeSavedQueue({ waiting: s.queueRef.current, current: null });
  }

  // Việc vừa lỗi vẫn nằm ở ô "đang chạy". Chạy video khác thì thả nó ra; để yên thì takeActive
  // xếp nó lại đầu hàng và nó chạy lại, lỗi lại. Lượt lỗi vẫn còn trong lịch sử để xếp lại khi muốn.
  function dropFailedActive(next?: Pick<JobSpec, "mode" | "source">) {
    const active = s.activeJob.current;
    const last = s.queueHistoryRef.current[s.queueHistoryRef.current.length - 1];
    if (!active || last?.state !== "failed" || last.job.id !== active.id) return;
    if (next && queueKey(next.mode, next.source) === queueKey(active.mode, active.source)) return;
    releaseRunning(active);
  }

  function jobSnapshot(jobMode: "file" | "link", source: string): QueueJob {
    return {
      id: `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`,
      label: queueLabel(jobMode, source),
      mode: jobMode,
      source,
      folder: s.folder,
      target: s.target,
      voice: s.voice,
      audioOnly: s.audioOnly,
      skipTranslate: s.skipTranslate,
      burnCaptions: s.burnCaptions,
      textOnly: s.textOnly,
      translation: translationSnapshot(),
      render: s.render,
    };
  }

  function translationSnapshot() {
    return { provider: s.provider, model: s.model || MODEL_HINTS[s.provider]?.fallback || "", useTerms: s.useTerms };
  }

  // Video đang chạy được biết bằng cả link gốc lẫn file đã tải từ link đó.
  function activeQueueKeys() {
    if (!s.running) return [];
    const keys: string[] = [];
    const active = s.activeJob.current;
    if (active) keys.push(queueKey(active.mode, active.source));
    if (s.downloading && s.link.trim()) keys.push(queueKey("link", s.link));
    if (s.video) keys.push(queueKey("file", s.video));
    return keys;
  }

  function pushJobs(items: QueueJob[]) {
    if (!s.folder) {
      s.setQueueNote("Chưa chọn folder kết quả.");
      return 0;
    }
    const result = appendJobs(s.queueRef.current, items, activeQueueKeys());
    rememberQueue(result.queue);
    if (result.added.length === 1) s.setQueueNote(`Đã thêm ${result.added[0].label} vào hàng.`);
    else if (result.added.length > 1) s.setQueueNote(`Đã thêm ${result.added.length} video vào hàng.`);
    else s.setQueueNote("Video này đang chạy hoặc đã có trong hàng.");
    void nameQueuedLinks(result.added);
    return result.added.length;
  }

  async function nameQueuedLinks(added: QueueJob[]) {
    await Promise.all(
      added
        .filter((job) => job.mode === "link")
        .map(async (job) => {
          try {
            const title = (await invoke<string>("link_title", { url: job.source })).trim();
            if (!title) return;
            if (s.activeJob.current?.id === job.id) {
              s.activeJob.current = { ...s.activeJob.current, label: title };
              writeSavedQueue({ waiting: s.queueRef.current, current: s.activeJob.current });
            }
            const current = s.queueRef.current;
            if (!current.some((item) => item.id === job.id)) return;
            rememberQueue(current.map((item) => (item.id === job.id ? { ...item, label: title } : item)));
          } catch {
            // Giữ mã video khi không đọc được tên.
          }
        }),
    );
  }

  async function enqueueCompose() {
    const source = s.mode === "file" ? s.composeVideo.trim() : s.composeLink.trim();
    if (!source || !s.folder) return;
    if (s.mode === "link") {
      const problem = await linkProblem(source);
      s.setLinkError(problem);
      if (problem) return;
    }
    const added = pushJobs([jobSnapshot(s.mode, source)]);
    if (added > 0) {
      if (s.mode === "file") s.setComposeVideo("");
      else s.setComposeLink("");
    }
  }

  function dropQueued(id: string) {
    rememberQueue(s.queueRef.current.filter((item) => item.id !== id));
    s.setQueueNote("");
  }

  function updateQueued(id: string) {
    rememberQueue(s.queueRef.current.map((item) => item.id === id
      ? { ...jobSnapshot(item.mode, item.source), id: item.id, label: item.label }
      : item));
    s.setQueueNote("Đã cập nhật video trong hàng theo lựa chọn hiện tại.");
  }

  async function pickVideo() {
    try {
      const selected = await open({
        multiple: true,
        directory: false,
        title: "Chọn video",
        filters: [{ name: "Video", extensions: ["mp4", "mkv", "mov", "webm", "avi", "m4v"] }],
      });
      if (selected == null) return;
      const paths = (Array.isArray(selected) ? selected : [selected]).filter(
        (item): item is string => typeof item === "string" && item !== "",
      );
      if (paths.length === 0) return;
      // The Windows dialog can still accept a manually typed non-video path.
      // Validate before changing the source or enqueuing any of the selection.
      const briefs = await Promise.all(paths.map(path => invoke<{width:number; height:number; duration:number}>("video_brief", { path })));
      s.setError("");
      if (s.running) {
        pushJobs(paths.map((path) => jobSnapshot("file", path)));
        return;
      }
      s.setComposeVideo(paths[0]);
      s.setSourceBrief({ ...briefs[0], path: paths[0] });
      s.setResult("");
      if (paths.length > 1) pushJobs(paths.slice(1).map((path) => jobSnapshot("file", path)));
    } catch (caught) {
      s.setError(errorText(caught));
    }
  }

  async function pickFolder() {
    s.setPrefsNote("");
    try {
      const selected = await open({ directory: true, multiple: false });
      if (typeof selected !== "string") return;
      const prefs = await invoke<Prefs>("save_prefs", { outputDir: selected });
      s.setFolder(prefs.outputDir);
      s.setResult("");
      s.setError("");
    } catch (caught) {
      s.setPrefsNote(errorText(caught));
    }
  }

  async function chooseTerms(on: boolean) {
    s.setPrefsNote("");
    s.setUseTerms(on);
    try {
      const prefs = await invoke<Prefs>("save_prefs", { useTerms: on });
      s.setUseTerms(prefs.useTerms);
    } catch (caught) {
      s.setUseTerms(!on);
      s.setPrefsNote(errorText(caught));
    }
  }

  async function chooseSkip(on: boolean) {
    s.setPrefsNote("");
    s.setSkipTranslate(on);
    try {
      const prefs = await invoke<Prefs>("save_prefs", { skipTranslate: on });
      s.setSkipTranslate(prefs.skipTranslate);
    } catch (caught) {
      s.setSkipTranslate(!on);
      s.setPrefsNote(errorText(caught));
    }
  }

  function chooseAudioOnly(on: boolean) {
    s.setAudioOnly(on);
    if (on) s.setTextOnly(false);
  }

  function chooseTextOnly(on: boolean) {
    s.setTextOnly(on);
    if (on) s.setAudioOnly(false);
  }

  async function chooseBurn(on: boolean) {
    s.setPrefsNote("");
    s.setBurnCaptions(on);
    try {
      const prefs = await invoke<Prefs>("save_prefs", { burnCaptions: on });
      s.setBurnCaptions(prefs.burnCaptions);
    } catch (caught) {
      s.setBurnCaptions(!on);
      s.setPrefsNote(errorText(caught));
    }
  }

  async function existingResult(
    source: string,
    spec: Pick<JobSpec, "folder" | "textOnly" | "target" | "voice" | "audioOnly" | "burnCaptions" | "skipTranslate" | "translation" | "render">,
  ) {
    if (!source || !spec.folder) return "";
    try {
      return await invoke<string>("finished_result", {
        video: source,
        outputDir: spec.folder,
        textOnly: spec.textOnly,
        target: spec.target,
        voice: spec.voice,
        audioOnly: spec.audioOnly,
        burn: spec.burnCaptions,
        skipTranslate: spec.skipTranslate,
        useTerms: spec.translation?.useTerms,
        translation: spec.translation,
        render: spec.render ?? DEFAULT_RENDER,
      });
    } catch {
      return "";
    }
  }

  function closePreview() {
    s.previewSlot.current = { open: false, listen: false, epoch: s.previewSlot.current.epoch + 1 };
    s.setReturnPreview(false);
  }

  function armPreview(listen: boolean) {
    s.previewSlot.current = { open: true, listen, epoch: s.previewSlot.current.epoch };
    s.setReturnPreview(true);
    s.setReturnListen(listen);
  }

  function openFinished(source: string, path: string, textOnly = false) {
    s.setVideo(source);
    s.setMode("file");
    s.setResult(path);
    s.setResultTextOnly(textOnly);
    s.setRunNotice(`Đã có kết quả của ${fileName(source)}. Đang mở để bạn xem.`);
    s.setPlayingKept("");
    s.setAgainOpen(false);
    s.setError("");
    s.setDownloading(false);
    s.setRunning(false);
    s.ownsRun.current = false;
    s.setView("watch");
  }

  // Chạy một video. Trả về true khi xong và hàng đợi còn được chạy tiếp.
  function requireActiveRequest() {
    if (s.holdQueue.current || s.skipActive.current) throw "Đã dừng.";
  }

  async function execute(spec: JobSpec, label = "", id = "") {
    // Legacy queues had no service snapshot. Freeze the current choice before
    // starting, and persist it so retries/reloads cannot silently switch later.
    spec = { ...spec, translation: spec.translation ?? translationSnapshot(), render: cleanRender(spec.render) };
    let proceed = false;
    const attemptStarted=Date.now();
    s.skipActive.current=false;
    noteRunning(spec, label, id);
    closePreview();
    s.retryRef.current = spec;
    s.setError("");
    s.setQueueNote("");
    s.setResult("");
    // Link thì gọi bằng nhãn trong hàng (trang · mã), không phải đoạn cuối của địa chỉ.
    s.setRunNotice(`Đã bắt đầu ${s.activeJob.current?.label || label || fileName(spec.source)}. Theo dõi xử lý tại Hàng đợi.`);
    s.setNameEdit(false);
    s.setAgainOpen(false);
    s.setPlayingKept("");
    s.setSteps(freshSteps(spec.audioOnly, spec.textOnly));
    s.setRunStartedAt(null);
    s.setRunning(true);
    s.setView("progress");
    s.runEpoch.current += 1; s.ownsRun.current = true;
    if (spec.mode === "link") s.setLink(spec.source);
    try {
      if (spec.voice) await invoke("save_voice", { lang: spec.target, name: spec.voice });
      requireActiveRequest();
      let source = spec.source;
      if (spec.mode === "link") {
        s.setDownload({ detail: spec.audioOnly ? "Đang lấy phụ đề." : "Đang chuẩn bị tải.", progress: null });
        s.setDownloading(true);
        source = spec.audioOnly
          ? await invoke<string>("prepare_listen", { url: spec.source, outputDir: spec.folder })
          : await invoke<string>("download_video", { url: spec.source, outputDir: spec.folder });
        requireActiveRequest();
        s.setDownloading(false);
        s.retryRef.current = { ...spec, mode: "file", source };
      }
      if (!source) return false;
      s.setVideo(source);
      const existing = await existingResult(source, spec);
      requireActiveRequest();
      if (existing) {
        recordRun(spec,"done",attemptStarted,existing);
        proceed = true;
        const more = s.queueRef.current.length > 0 && !s.holdQueue.current;
        if (more) {
          s.setQueueNote(`${fileName(source)} đã có bản lồng, chuyển video sau.`);
          return true;
        }
        openFinished(source, existing, spec.textOnly);
        return false;
      }
      const started = Date.now();
      s.setNow(started);
      s.setRunStartedAt(started);
      const path = await invoke<string>("start_dub", {
        video: source,
        outputDir: spec.folder,
        target: spec.target,
        redo: false,
        burn: spec.textOnly || spec.audioOnly ? false : spec.burnCaptions,
        audioOnly: spec.audioOnly,
        textOnly: spec.textOnly,
        skipTranslate: spec.skipTranslate,
        translation: spec.translation,
        render: spec.render,
      });
      recordRun(spec,"done",attemptStarted,path);
      s.setResult(path);
      s.setResultTextOnly(spec.textOnly);
      proceed = !s.holdQueue.current;
      s.setRunNotice(proceed && s.queueRef.current.length > 0
        ? `Đã xong ${label || fileName(source)}. Đang chuyển sang video tiếp theo.`
        : `Đã xử lý xong ${label || fileName(source)}${id && s.queueRef.current.length === 0 ? ". Hàng đợi đã hoàn tất" : ""}. Đang mở kết quả để bạn xem.`);
      s.setView(proceed && s.queueRef.current.length > 0 ? "progress" : "watch");
      return proceed;
    } catch (caught) {
      const message=errorText(caught);
      // Lý do chỉ ghi một chỗ, ở s.error. Thông báo chỉ nói việc đã dừng.
      s.setRunNotice(message === "Đã dừng." ? "Đã dừng xử lý." : "Xử lý bị gián đoạn.");
      if(s.skipActive.current){
        s.skipActive.current=false;recordRun(spec,"skipped",attemptStarted);proceed=!s.holdQueue.current;
      }else{
        s.setError(message);
        if(message!=="Đã dừng.")recordRun(spec,"failed",attemptStarted,"",message);
        proceed=message!=="Đã dừng."&&s.queuePolicy==="continue"&&!s.holdQueue.current;
      }
      s.setView("progress");
      return proceed;
    } finally {
      if (proceed) releaseRunning(spec);
      s.ownsRun.current = false;
      s.setDownloading(false);
      s.setNow(Date.now());
      s.setNameReview(null);
      if (!(proceed && s.queueRef.current.length > 0 && !s.holdQueue.current)) s.setRunning(false);
    }
  }

  async function pump() {
    while (!s.holdQueue.current && s.queueRef.current.length > 0) {
      const next = s.queueRef.current[0];
      const spec: JobSpec = {
        mode: next.mode,
        source: next.source,
        folder: next.folder,
        target: next.target,
        voice: next.voice,
        audioOnly: next.audioOnly,
        skipTranslate: next.skipTranslate,
        burnCaptions: next.burnCaptions,
        textOnly: next.textOnly,
        translation: next.translation,
        render: next.render,
      };
      const ok = await execute(spec, next.label, next.id);
      if (!ok) return;
    }
  }

  async function runFrom(source: string) {
    if (s.gate.current) return;
    s.gate.current = true;
    s.holdQueue.current = false;
    try {
      dropFailedActive({ mode: "file", source });
      const ok = await execute({
        mode: "file",
        source,
        folder: s.folder,
        target: s.target,
        voice: s.voice,
        audioOnly: s.audioOnly,
        skipTranslate: s.skipTranslate,
        render: s.render,
        burnCaptions: s.burnCaptions,
        textOnly: s.textOnly,
        translation: translationSnapshot(),
      });
      if (ok) await pump();
    } finally {
      s.gate.current = false;
    }
  }

  async function runQueue() {
    if (s.running || s.gate.current || s.queueRef.current.length === 0) return;
    s.gate.current = true;
    s.holdQueue.current = false;
    try {
      // Continuing the waiting list is not a retry of the failed current job.
      // Keep that attempt in history; otherwise takeActive puts it back in line.
      dropFailedActive();
      await pump();
    } finally {
      s.gate.current = false;
    }
  }

  async function run() {
    if (s.running) {
      await enqueueCompose();
      return;
    }
    if (s.gate.current) return;
    const source = s.mode === "file" ? s.composeVideo.trim() : s.composeLink.trim();
    if (!source) {
      await runQueue();
      return;
    }
    s.gate.current = true;
    s.holdQueue.current = false;
    try {
      // Link sai thì dừng ở ô nhập: ô giữ nguyên để sửa, không chạy, không xếp hàng, không ghi lịch sử.
      if (s.mode === "link") {
        const problem = await linkProblem(source);
        s.setLinkError(problem);
        if (problem) return;
      }
      if (s.mode === "file") s.setComposeVideo("");
      else s.setComposeLink("");
      dropFailedActive({ mode: s.mode, source });
      const ok = await execute({
        mode: s.mode,
        source,
        folder: s.folder,
        target: s.target,
        voice: s.voice,
        audioOnly: s.audioOnly,
        skipTranslate: s.skipTranslate,
        render: s.render,
        burnCaptions: s.burnCaptions,
        textOnly: s.textOnly,
        translation: translationSnapshot(),
      });
      if (ok) await pump();
    } finally {
      s.gate.current = false;
    }
  }

  async function retryLast() {
    const spec = s.retryRef.current;
    if (!spec || s.running || s.gate.current) return;
    s.gate.current = true;
    s.holdQueue.current = false;
    try {
      // Chạy lại đúng việc vừa lỗi: giữ nguyên mã và tên để không xếp thêm một bản vào hàng.
      const failed = s.activeJob.current;
      const ok = await execute(spec, failed?.label ?? "", failed?.id ?? "");
      if (ok) await pump();
    } finally {
      s.gate.current = false;
    }
  }

  async function fetchVideo() {
    if (!s.result || s.running || s.gate.current) return;
    s.gate.current = true; s.holdQueue.current = false; s.skipActive.current = false;
    closePreview();
    s.setError("");
    s.setOpenNote("");
    s.setSteps(freshSteps(false));
    s.setRunning(true);
    s.setView("progress");
    s.runEpoch.current += 1; s.ownsRun.current = true;
    s.setDownload({ detail: "Đang tải video.", progress: null });
    s.setDownloading(true);
    try {
      const job = await invoke<string>("download_saved_video", { result: s.result, outputDir: s.folder });
      requireActiveRequest();
      s.setDownloading(false);
      const started = Date.now();
      s.setNow(started);
      s.setRunStartedAt(started);
      const path = await invoke<string>("start_dub", {
        video: job,
        outputDir: s.folder,
        target: s.target,
        redo: false,
        burn: s.burnCaptions,
        audioOnly: false,
        textOnly: false,
        skipTranslate: s.skipTranslate,
        render: s.render,
      });
      s.setResult(path);
      s.setResultTextOnly(false);
      s.setView("watch");
    } catch (caught) {
      s.setError(errorText(caught));
      s.setView("progress");
    } finally {
      s.gate.current = false;
      s.ownsRun.current = false;
      s.setDownloading(false);
      s.setNow(Date.now());
      s.setRunning(false);
    }
  }

  async function runLive() {
    if (s.gate.current || s.running) return;
    s.gate.current = true; s.holdQueue.current = false; s.skipActive.current = false;
    s.setError(""); s.setSteps(freshSteps(s.audioOnly, s.textOnly));
    s.setRunning(true); s.runEpoch.current += 1; s.ownsRun.current = true; s.setView("progress");
    try {
      let source = s.composeVideo;
      if (s.mode === "link") {
        s.setError("");
        s.setResult("");
        s.setSteps(freshSteps(s.audioOnly, s.textOnly));
        s.setRunning(true);
        s.setView("progress");
        s.runEpoch.current += 1; s.ownsRun.current = true;
        s.setDownload({ detail: s.audioOnly ? "Đang lấy phụ đề." : "Đang chuẩn bị tải.", progress: null });
        s.setDownloading(true);
        source = s.audioOnly
          ? await invoke<string>("prepare_listen", { url: s.composeLink.trim(), outputDir: s.folder })
          : await invoke<string>("download_video", { url: s.composeLink.trim(), outputDir: s.folder });
        requireActiveRequest();
        if (s.audioOnly) {
          const outline = await invoke<{ count: number }>("listen_part", { root: source, part: 1 });
          requireActiveRequest();
          if (outline.count > 0) {
            s.setVideo(source);
            s.setComposeVideo(source);
            s.setPartsRoot(source);
            s.setDownloading(false);
            s.setView("parts");
            return;
          }
        }
        s.setVideo(source);
        s.setComposeVideo(source);
        s.setDownloading(false);
      }
      if (!source) return;
      const existing = await existingResult(source, {
        folder: s.folder,
        textOnly: s.textOnly,
        target: s.target,
        voice: s.voice,
        audioOnly: s.audioOnly,
        burnCaptions: s.burnCaptions,
        skipTranslate: s.skipTranslate,
        render: s.render,
      });
      requireActiveRequest();
      if (existing) {
        openFinished(source, existing, s.textOnly);
        return;
      }
      s.setVideo(source);
      s.setMode("file");
      s.setError("");
      s.watchUpdateRef.current = null;
      s.setWatchUpdate(null);
      armPreview(s.audioOnly);
      s.setView("live");
      s.setRunning(true);
      s.runEpoch.current += 1; s.ownsRun.current = true;
      await invoke("start_watch", {
        video: source,
        outputDir: s.folder,
        target: s.target,
        skipTranslate: s.skipTranslate,
        textOnly: s.textOnly,
      });
    } catch (caught) {
      const message = errorText(caught);
      s.setError(message);
      if (s.watchUpdateRef.current) {
        const next: WatchUpdate = {
          ...s.watchUpdateRef.current,
          running: false,
          published: false,
          lines: [],
          readyUntil: 0,
          detail: message,
        };
        s.watchUpdateRef.current = next;
        s.setWatchUpdate(next);
        if (s.viewRef.current === "live") s.setView("live");
      } else if (s.viewRef.current === "live") {
        s.setView("progress");
      }
    } finally {
      s.ownsRun.current = false;
      s.setDownloading(false);
      s.setRunning(false);
      s.gate.current = false;
    }
  }

  async function resume(item: FinishedDub) {
    if (item.done) return;
    if (!item.video) {
      s.setError("Không thấy video gốc để chạy tiếp.");
      s.setView("progress");
      return;
    }
    if (s.running) {
      pushJobs([jobSnapshot("file", item.video)]);
      return;
    }
    s.setVideo(item.video);
    s.setMode("file");
    await runFrom(item.video);
  }

  async function stop() {
    s.holdQueue.current = true;
    s.setRunNotice("Đang dừng… Chờ tác vụ hiện tại kết thúc trước khi chạy việc mới.");
    await invoke("cancel_dub");
  }

  return {
    chooseQueuePolicy,bulkPreset,retryRecord,skipCurrent,moveQueued:(id:string,delta:number)=>rememberQueue(moveWaiting(s.queueRef.current,id,delta)),
    renamePreset: (id: string, name: string) => storePresets(s.presets.map(p => p.id === id ? { ...p, name: name.trim().slice(0,60) } : p)),
    chooseRender, savePreset, applyPreset, deletePreset: (id: string) => storePresets(s.presets.filter((p) => p.id !== id)),
    rememberQueue,
    pickVideo,
    pickFolder,
    chooseTerms,
    chooseSkip,
    chooseBurn,
    chooseAudioOnly,
    chooseTextOnly,
    dropQueued,
    updateQueued,
    runFrom,
    runQueue,
    run,
    retryLast,
    fetchVideo,
    runLive,
    resume,
    stop,
  };
}
