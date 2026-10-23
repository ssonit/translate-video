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

export function jobActions(s: Session) {
  function rememberQueue(next: QueueJob[]) {
    s.queueRef.current = next;
    s.setQueue(next);
    writeSavedQueue({ waiting: next, current: s.activeJob.current });
  }

  function noteRunning(spec: JobSpec, label = "", id = "") {
    const same =
      s.activeJob.current &&
      queueKey(s.activeJob.current.mode, s.activeJob.current.source) === queueKey(spec.mode, spec.source);
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
    };
  }

  function activeQueueKey() {
    if (!s.running) return "";
    if (s.downloading && s.link.trim()) return queueKey("link", s.link);
    if (s.video) return queueKey("file", s.video);
    return "";
  }

  function pushJobs(items: QueueJob[]) {
    if (!s.folder) {
      s.setQueueNote("Chưa chọn folder kết quả.");
      return 0;
    }
    const result = appendJobs(s.queueRef.current, items, activeQueueKey());
    rememberQueue(result.queue);
    if (result.added.length === 1) s.setQueueNote(`Đã thêm ${result.added[0].label} vào hàng.`);
    else if (result.added.length > 1) s.setQueueNote(`Đã thêm ${result.added.length} video vào hàng.`);
    else s.setQueueNote("Video này đã có trong hàng.");
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

  function enqueueCompose() {
    const source = s.mode === "file" ? s.composeVideo.trim() : s.composeLink.trim();
    if (!source || !s.folder) return;
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
      s.setError("");
      if (s.running) {
        pushJobs(paths.map((path) => jobSnapshot("file", path)));
        return;
      }
      s.setComposeVideo(paths[0]);
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

  async function existingResult(source: string, outputDir: string, textOnly: boolean) {
    if (!source || !outputDir) return "";
    try {
      return await invoke<string>("finished_result", { video: source, outputDir, textOnly });
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
    s.setPlayingKept("");
    s.setAgainOpen(false);
    s.setError("");
    s.setDownloading(false);
    s.setRunning(false);
    s.ownsRun.current = false;
    s.setView("watch");
  }

  // Chạy một video. Trả về true khi xong và hàng đợi còn được chạy tiếp.
  async function execute(spec: JobSpec, label = "", id = "") {
    let proceed = false;
    noteRunning(spec, label, id);
    closePreview();
    s.retryRef.current = spec;
    s.setError("");
    s.setQueueNote("");
    s.setResult("");
    s.setNameEdit(false);
    s.setAgainOpen(false);
    s.setPlayingKept("");
    s.setSteps(freshSteps(spec.audioOnly, spec.textOnly));
    s.setRunStartedAt(null);
    s.setRunning(true);
    s.setView("progress");
    s.ownsRun.current = true;
    if (spec.mode === "link") s.setLink(spec.source);
    try {
      if (spec.voice) await invoke("save_voice", { lang: spec.target, name: spec.voice });
      let source = spec.source;
      if (spec.mode === "link") {
        s.setDownload({ detail: spec.audioOnly ? "Đang lấy phụ đề." : "Đang chuẩn bị tải.", progress: null });
        s.setDownloading(true);
        source = spec.audioOnly
          ? await invoke<string>("prepare_listen", { url: spec.source, outputDir: spec.folder })
          : await invoke<string>("download_video", { url: spec.source, outputDir: spec.folder });
        s.setDownloading(false);
        s.retryRef.current = { ...spec, mode: "file", source };
      }
      if (!source) return false;
      s.setVideo(source);
      const existing = await existingResult(source, spec.folder, spec.textOnly);
      if (existing) {
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
      });
      s.setResult(path);
      s.setResultTextOnly(spec.textOnly);
      proceed = !s.holdQueue.current;
      s.setView(proceed && s.queueRef.current.length > 0 ? "progress" : "watch");
      return proceed;
    } catch (caught) {
      s.setError(errorText(caught));
      s.setView("progress");
      return false;
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
      const ok = await execute({
        mode: "file",
        source,
        folder: s.folder,
        target: s.target,
        voice: s.voice,
        audioOnly: s.audioOnly,
        skipTranslate: s.skipTranslate,
        burnCaptions: s.burnCaptions,
        textOnly: s.textOnly,
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
      await pump();
    } finally {
      s.gate.current = false;
    }
  }

  async function run() {
    if (s.running) {
      enqueueCompose();
      return;
    }
    if (s.gate.current) return;
    const source = s.mode === "file" ? s.composeVideo.trim() : s.composeLink.trim();
    if (!source) {
      await runQueue();
      return;
    }
    if (s.mode === "file") s.setComposeVideo("");
    else s.setComposeLink("");
    s.gate.current = true;
    s.holdQueue.current = false;
    try {
      const ok = await execute({
        mode: s.mode,
        source,
        folder: s.folder,
        target: s.target,
        voice: s.voice,
        audioOnly: s.audioOnly,
        skipTranslate: s.skipTranslate,
        burnCaptions: s.burnCaptions,
        textOnly: s.textOnly,
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
      const ok = await execute(spec);
      if (ok) await pump();
    } finally {
      s.gate.current = false;
    }
  }

  async function fetchVideo() {
    if (!s.result) return;
    closePreview();
    s.setError("");
    s.setOpenNote("");
    s.setSteps(freshSteps(false));
    s.setRunning(true);
    s.setView("progress");
    s.ownsRun.current = true;
    s.setDownload({ detail: "Đang tải video.", progress: null });
    s.setDownloading(true);
    try {
      const job = await invoke<string>("download_saved_video", { result: s.result, outputDir: s.folder });
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
      });
      s.setResult(path);
      s.setResultTextOnly(false);
      s.setView("watch");
    } catch (caught) {
      s.setError(errorText(caught));
      s.setView("progress");
    } finally {
      s.ownsRun.current = false;
      s.setDownloading(false);
      s.setNow(Date.now());
      s.setRunning(false);
    }
  }

  async function runLive() {
    try {
      let source = s.composeVideo;
      if (s.mode === "link") {
        s.setError("");
        s.setResult("");
        s.setSteps(freshSteps(s.audioOnly, s.textOnly));
        s.setRunning(true);
        s.setView("progress");
        s.ownsRun.current = true;
        s.setDownload({ detail: s.audioOnly ? "Đang lấy phụ đề." : "Đang chuẩn bị tải.", progress: null });
        s.setDownloading(true);
        source = s.audioOnly
          ? await invoke<string>("prepare_listen", { url: s.composeLink.trim(), outputDir: s.folder })
          : await invoke<string>("download_video", { url: s.composeLink.trim(), outputDir: s.folder });
        if (s.audioOnly) {
          const outline = await invoke<{ count: number }>("listen_part", { root: source, part: 1 });
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
        s.ownsRun.current = false;
      }
      if (!source) return;
      const existing = await existingResult(source, s.folder, s.textOnly);
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
      s.ownsRun.current = true;
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
    await invoke("cancel_dub");
  }

  return {
    rememberQueue,
    pickVideo,
    pickFolder,
    chooseTerms,
    chooseSkip,
    chooseBurn,
    chooseAudioOnly,
    chooseTextOnly,
    dropQueued,
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
