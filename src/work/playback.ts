import { invoke } from "@tauri-apps/api/core";
import { ask, save } from "@tauri-apps/plugin-dialog";
import {
  againPlan,
  errorText,
  fileName,
  freshSteps,
  isAudio,
  isCaptioned,
  showAsTextOnly,
  jobFolder,
  keptLabel,
  nameRowError,
  openJobFolder,
  plainVideo,
  sameFile,
  videoToSave,
} from "./format";
import type { Session } from "./state";
import type { DubStatus, FinishedDub, KeptVersion, NameList, Prefs } from "./types";

export function playbackActions(s: Session) {
  function watched() {
    return s.dubs.find(
      (item) => sameFile(item.path, s.result) || sameFile(item.path, plainVideo(s.result)),
    );
  }

  function applyStatus(status: DubStatus) {
    const stamp = Date.now();
    s.setSteps(
      freshSteps(status.audioOnly, status.textOnly).map((step) => {
        const saved = status.steps.find((item) => item.id === step.id);
        if (!saved) return step;
        return {
          ...step,
          label: saved.label || step.label,
          state: saved.state,
          detail: saved.detail,
          progress: saved.progress,
          startedAt: saved.state === "running" ? stamp : null,
        };
      }),
    );
    s.setRunning(status.running);
    s.setDownloading(status.downloading);
    if (status.download) s.setDownload(status.download);
    s.setRunStartedAt(status.running && !status.downloading ? status.startedMs : null);
    s.setNow(stamp);
    if (status.video) s.setVideo(status.video);
    if (status.outputDir) s.setFolder(status.outputDir);
    s.setResult(status.result ?? "");
    s.setResultTextOnly(status.textOnly === true && isCaptioned(status.result ?? ""));
    s.setError(status.error ?? "");
    if (status.names) {
      s.setNameEdit(false);
      s.setNameReview(status.names);
      s.setNameRows(status.names.names.length ? status.names.names : [{ from: "", to: "" }]);
      s.setNameNote("");
    } else {
      s.setNameReview(null);
    }
  }

  function applyPrefs(prefs: Prefs, keepFolder: boolean) {
    s.setFolder((current) => (keepFolder && current ? current : prefs.outputDir));
    s.setUseTerms(prefs.useTerms);
    s.setBurnCaptions(prefs.burnCaptions);
    s.setSkipTranslate(prefs.skipTranslate);
  }

  function releasePlayer() {
    const node = s.mediaRef.current;
    if (!node) return;
    node.pause();
    node.removeAttribute("src");
    node.load();
  }

  async function openResult(path: string) {
    s.setOpenNote("");
    try {
      await invoke("open_result", { path });
    } catch (caught) {
      s.setOpenNote(errorText(caught));
    }
  }

  async function joinListen() {
    if (!s.result || s.joining || s.running || isAudio(s.result)) return;
    releasePlayer();
    s.setPlayable(false);
    s.setOpenNote("");
    s.setJoinProgress(null);
    s.setJoining(true);
    try {
      const path = await invoke<string>("join_listen", { path: s.result });
      s.setListenFile(path);
      s.setResultEpoch((epoch) => epoch + 1);
    } catch (caught) {
      s.setOpenNote(errorText(caught));
      invoke("allow_result", { path: s.result })
        .then(() => s.setPlayable(true))
        .catch(() => {});
    } finally {
      s.setJoining(false);
      s.setJoinProgress(null);
    }
  }

  function showVideo() {
    if (!s.listenFile) return;
    releasePlayer();
    s.setListenFile("");
    s.setResultEpoch((epoch) => epoch + 1);
  }

  async function downloadAudio() {
    const source = s.result;
    if (!source || s.savingAudio) return;
    s.setOpenNote("");
    const base = fileName(isAudio(source) ? source : plainVideo(source))
      .replace(/\.chu\.mp4$/i, ".mp4")
      .replace(/\.(mp4|m4a)$/i, "");
    try {
      const picked = await save({
        title: "Lưu audio bản dịch",
        defaultPath: `${base}.m4a`,
        filters: [{ name: "Audio", extensions: ["m4a"] }],
      });
      if (typeof picked !== "string" || picked === "") return;
      s.setSavingAudio(true);
      await invoke("save_dub_audio", { path: source, dest: picked });
    } catch (caught) {
      s.setOpenNote(errorText(caught));
    } finally {
      s.setSavingAudio(false);
    }
  }

  async function downloadVideo() {
    const source = videoToSave(s.result, s.playingKept);
    if (!source || s.savingVideo) return;
    s.setOpenNote("");
    try {
      const picked = await save({
        title: "Lưu video",
        defaultPath: fileName(source),
        filters: [{ name: "Video", extensions: ["mp4"] }],
      });
      if (typeof picked !== "string" || picked === "") return;
      s.setSavingVideo(true);
      await invoke("save_dub_video", { path: source, dest: picked });
    } catch (caught) {
      s.setOpenNote(errorText(caught));
    } finally {
      s.setSavingVideo(false);
    }
  }

  function chooseTarget(code: string) {
    s.setTarget(code);
    try {
      localStorage.setItem("target-language", code);
    } catch {
      // Không lưu được thì lần sau về tiếng Việt.
    }
  }

  async function chooseVoice(name: string) {
    s.setVoiceNote("");
    // Giọng đang đọc lấy file đã lưu lúc bắt đầu. Đổi ở đây chỉ gắn vào video xếp sau.
    if (s.running) {
      s.setVoice(name);
      return;
    }
    try {
      await invoke("save_voice", { lang: s.target, name });
      s.setVoice(name);
    } catch (caught) {
      s.setVoiceNote(errorText(caught));
    }
  }

  async function previewVoice() {
    s.setVoiceNote("");
    s.setPreviewing(true);
    try {
      const wav = await invoke<ArrayBuffer>("preview_voice", { lang: s.target, name: s.voice });
      const url = URL.createObjectURL(new Blob([wav], { type: "audio/wav" }));
      const audio = new Audio(url);
      audio.onended = () => URL.revokeObjectURL(url);
      await audio.play();
    } catch (caught) {
      s.setVoiceNote(errorText(caught));
    } finally {
      s.setPreviewing(false);
    }
  }

  async function loadChoice() {
    try {
      const choice = await invoke<{ provider: string; model?: string }>("get_translation_choice");
      s.setProvider(choice.provider);
      s.setModel(choice.model ?? "");
      s.setCustomModel(false);
    } catch {
      // Chạy bằng trang Vite một mình thì không đọc được; giữ giá trị mặc định.
    }
  }

  async function chooseProvider(next: string) {
    if (next === s.provider && s.pending === "") return;
    s.setModelNote("");
    s.setPending("");
    try {
      await invoke("switch_translation_provider", { provider: next });
      await loadChoice();
    } catch (caught) {
      // Chưa lưu key cho dịch vụ này: hiện ô nhập key ngay tại đây.
      s.setPending(next);
      s.setKeyDraft("");
      s.setModelNote(errorText(caught));
    }
  }

  async function saveKey() {
    try {
      await invoke("save_translation_choice", { provider: s.pending, apiKey: s.keyDraft });
      s.setPending("");
      s.setKeyDraft("");
      s.setModelNote("Đã lưu key trên máy.");
      await loadChoice();
    } catch (caught) {
      s.setModelNote(errorText(caught));
    }
  }

  async function copyModelCommand(command: string) {
    await navigator.clipboard.writeText(command);
    s.setCopied(command);
    window.setTimeout(() => s.setCopied(""), 1600);
  }

  async function saveModel(value = s.model) {
    s.setModelNote("");
    try {
      await invoke("save_translation_model", { model: value.trim() });
      s.setModelNote("Đã lưu.");
    } catch (caught) {
      s.setModelNote(errorText(caught));
    }
  }

  function againSummary() {
    const current = watched();
    return againPlan({
      retranscribe: s.retranscribe,
      lang: s.againLang,
      currentLang: current?.language || "",
      voice: s.againVoice,
      savedVoice: s.againSavedVoice,
      names: s.againNames,
      base: s.againBase,
      textOnly: s.againTextOnly,
      wasTextOnly: s.againTextBase,
    });
  }

  async function openAgain() {
    const root = openJobFolder(s.result);
    if (!root || s.running) return;
    s.setAgainNote("");
    s.setRetranscribe(false);
    s.setAgainLang(watched()?.language || "vi");
    const text = isCaptioned(s.result);
    s.setAgainTextOnly(text);
    s.setAgainTextBase(text);
    try {
      const list = await invoke<NameList>("read_names", { root });
      const rows = list.names.length ? list.names : [{ from: "", to: "" }];
      s.setAgainNames(rows);
      s.setAgainBase(rows);
      s.setAgainOpen(true);
    } catch (caught) {
      s.setOpenNote(errorText(caught));
    }
  }

  async function runAgain() {
    if (!s.result || s.running) return;
    const unchanged = againSummary() === "Chưa đổi ngôn ngữ, giọng hay tên.";
    if (!s.retranscribe) {
      const problem = nameRowError(s.againNames);
      if (problem) {
        s.setAgainNote(problem);
        return;
      }
    }
    if (unchanged) {
      s.setAgainNote("Chưa đổi ngôn ngữ, giọng hay tên.");
      return;
    }
    s.setAgainNote("");
    const agreed = await ask(
      `${againSummary()}\n\nVideo đang xem vẫn mở được trong app. File nội dung sẽ thành bản mới. Không cần tải về trước.`,
      { title: "Làm lại", kind: "warning", okLabel: "Làm lại", cancelLabel: "Huỷ" },
    );
    if (!agreed) return;
    if (s.retranscribe) {
      const slow = await ask(
        "Lần này chép lại lời từ đầu, sẽ lâu.\n\nKhông còn video gốc hoặc tiếng đã tải thì dừng, không xóa file.",
        { title: "Chép lại lời", kind: "warning", okLabel: "Chép lại", cancelLabel: "Huỷ" },
      );
      if (!slow) return;
    }
    const node = s.mediaRef.current;
    if (node) {
      node.pause();
      node.removeAttribute("src");
      node.load();
    }
    s.setPlayable(false);
    s.setError("");
    s.setAgainOpen(false);
    s.setPlayingKept("");
    s.setSteps(freshSteps(isAudio(s.result), s.againTextOnly));
    s.setRunStartedAt(Date.now());
    s.setRunning(true);
    s.setView("progress");
    s.ownsRun.current = true;
    try {
      const path = await invoke<string>("start_again", {
        result: s.result,
        target: s.againLang,
        voice: s.againVoice,
        names: s.againNames,
        retranscribe: s.retranscribe,
        burn: isAudio(s.result) || s.againTextOnly ? false : s.burnCaptions,
        audioOnly: isAudio(s.result),
        textOnly: s.againTextOnly && !isAudio(s.result),
      });
      s.setResult(path);
      s.setResultTextOnly(s.againTextOnly && !isAudio(path));
      s.setResultEpoch((epoch) => epoch + 1);
      s.setListTick((tick) => tick + 1);
      s.setView("watch");
    } catch (caught) {
      s.setAgainNote(errorText(caught));
      s.setAgainOpen(true);
      s.setView("watch");
      invoke("allow_result", { path: s.result })
        .then(() => s.setPlayable(true))
        .catch(() => {});
    } finally {
      s.ownsRun.current = false;
      s.setNow(Date.now());
      s.setRunning(false);
    }
  }

  async function deleteKept(item: KeptVersion) {
    const label = keptLabel(item);
    const agreed = await ask(
      `Xóa "${label}"?\n\nChỉ mất bản này. Bản đang dùng và các bản khác vẫn còn.`,
      { title: "Xóa bản cũ", kind: "warning", okLabel: "Xóa", cancelLabel: "Giữ lại" },
    );
    if (!agreed) return;
    s.setOpenNote("");
    try {
      await invoke("delete_kept_version", { outputDir: s.folder, path: item.path });
      if (sameFile(s.playingKept, item.path)) s.setPlayingKept("");
      s.setListTick((tick) => tick + 1);
    } catch (caught) {
      s.setOpenNote(errorText(caught));
    }
  }

  async function retakeLines(edits: { index: number; text: string }[]) {
    const source = s.result;
    if (!source || s.running || edits.length === 0) return;
    const node = s.mediaRef.current;
    if (node) {
      node.pause();
      node.removeAttribute("src");
      node.load();
    }
    s.setPlayable(false);
    s.setError("");
    s.setCaptionNote("");
    s.setSteps(freshSteps(isAudio(source), showAsTextOnly(source, watched()?.path ?? "", s.resultTextOnly)));
    s.setRunStartedAt(Date.now());
    s.setRunning(true);
    s.setView("progress");
    s.ownsRun.current = true;
    try {
      const path = await invoke<string>("revise_lines", { path: source, edits });
      s.setResult(path);
      s.setResultEpoch((epoch) => epoch + 1);
      s.setListTick((tick) => tick + 1);
      s.setView("watch");
    } catch (caught) {
      s.setError(errorText(caught));
      s.setView("watch");
      invoke("allow_result", { path: source })
        .then(() => s.setPlayable(true))
        .catch(() => {});
    } finally {
      s.ownsRun.current = false;
      s.setNow(Date.now());
      s.setRunning(false);
    }
  }

  async function addCaptions() {
    const source = plainVideo(s.result);
    if (!source || s.captioning) return;
    s.setCaptionNote("");
    s.setCaptioning(true);
    s.setCaptionProgress(null);
    try {
      const path = await invoke<string>("burn_captions", { path: source });
      s.setResult(path);
    } catch (caught) {
      s.setCaptionNote(errorText(caught));
    } finally {
      s.setCaptioning(false);
      s.setCaptionProgress(null);
    }
  }

  async function submitNames(keep: boolean) {
    if (!keep) {
      const problem = nameRowError(s.nameRows);
      if (problem) {
        s.setNameNote(problem);
        return;
      }
    }
    s.setNameSending(true);
    s.setNameNote("");
    try {
      await invoke("confirm_names", { keep, names: s.nameRows });
      s.setNameReview(null);
    } catch (caught) {
      s.setNameNote(errorText(caught));
    } finally {
      s.setNameSending(false);
    }
  }

  async function openNames(root: string) {
    if (!root || s.running) return;
    s.setNameNote("");
    try {
      const list = await invoke<NameList>("read_names", { root });
      s.setNameRoot(root);
      s.setNameRows(list.names.length ? list.names : [{ from: "", to: "" }]);
      s.setNameEdit(true);
    } catch (caught) {
      s.setError(errorText(caught));
    }
  }

  function closeNameEdit() {
    s.setNameEdit(false);
    s.setNameNote("");
  }

  async function saveEditedNames() {
    const problem = nameRowError(s.nameRows);
    if (problem) {
      s.setNameNote(problem);
      return;
    }
    s.setNameSending(true);
    s.setNameNote("");
    try {
      const changed = await invoke<boolean>("save_names", { root: s.nameRoot, names: s.nameRows });
      s.setListTick((tick) => tick + 1);
      if (changed) {
        s.setNameNote("Đã lưu.");
      } else {
        s.setNameNote("Đã lưu. Danh sách không đổi.");
      }
    } catch (caught) {
      s.setNameNote(errorText(caught));
    } finally {
      s.setNameSending(false);
    }
  }

  async function removeDub(item: FinishedDub) {
    if (s.removing) return;
    const agreed = await ask(
      `Xóa "${item.title}" khỏi danh sách?\n\nFolder việc này sẽ mất, gồm video đã lồng, audio, phụ đề và bản dịch. File gốc trên máy và video đã tải về vẫn còn.`,
      { title: "Xóa khỏi danh sách", kind: "warning", okLabel: "Xóa", cancelLabel: "Giữ lại" },
    );
    if (!agreed) return;
    s.setRemoving(item.path);
    s.setListNote("");
    try {
      await invoke("delete_dub", { outputDir: s.folder, path: item.path });
      const folderOf = jobFolder(item);
      if (sameFile(folderOf, openJobFolder(s.result)) || s.draft?.path === item.path) {
        s.setResult("");
        s.setResultTextOnly(false);
        s.setDraft(null);
        s.setLines([]);
        s.setOpenNote("");
        s.setCaptionNote("");
        s.setView("new");
      }
      s.setDubs((rows) => rows.filter((row) => !sameFile(jobFolder(row), folderOf)));
      s.setListTick((tick) => tick + 1);
    } catch (caught) {
      s.setListNote(typeof caught === "string" ? caught : "Không xóa được.");
    } finally {
      s.setRemoving("");
    }
  }

  return {
    applyStatus,
    applyPrefs,
    openResult,
    joinListen,
    showVideo,
    downloadAudio,
    downloadVideo,
    chooseTarget,
    chooseVoice,
    previewVoice,
    loadChoice,
    chooseProvider,
    saveKey,
    copyModelCommand,
    saveModel,
    againSummary,
    openAgain,
    runAgain,
    deleteKept,
    retakeLines,
    addCaptions,
    submitNames,
    openNames,
    closeNameEdit,
    saveEditedNames,
    removeDub,
  };
}
