import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { queueKey, readSavedQueue, writeSavedQueue, type QueueJob } from "../queue";
import { previewStillOpen } from "../watchPlay";
import type { WatchUpdate } from "../WatchLive";
import { errorText, isAudio } from "./format";
import type { Session } from "./state";
import { watchDataKey, watchMediaKey } from "./watchLoad";
import type { DubStatus, FinishedDub, KeptVersion, NamesPending, Prefs, Progress, ScriptLine, RunPause } from "./types";

export function useWorkEffects(
  s: Session,
  fx: {
    busyText: string;
    onBusy: (text: string) => void;
    prefsEpoch: number;
    shown: string;
    applyStatus: (status: DubStatus) => void;
    applyPrefs: (prefs: Prefs, keepFolder: boolean) => void;
    loadChoice: () => Promise<void>;
    rememberQueue: (next: QueueJob[]) => void;
  },
) {
  useEffect(() => {
    let live = true;
    invoke<DubStatus>("dub_status")
      .then((status) => {
        if (!live || s.ownsRun.current) return;
        if (!status.running && status.steps.length === 0 && !status.result && !status.error) return;
        fx.applyStatus(status);
        if (status.running) s.setView("progress");
      })
      .catch(() => {});
    const unlisten = listen<DubStatus>("dub-finished", (event) => {
      if (s.ownsRun.current) return;
      fx.applyStatus(event.payload);
      if (event.payload.result) {
        s.activeJob.current = null;
        writeSavedQueue({ waiting: s.queueRef.current, current: null });
        s.setView("watch");
      } else if (event.payload.error) s.setView("progress");
    });
    return () => {
      live = false;
      void unlisten.then((stop) => stop());
    };
    // The listener belongs to this window for its whole life. Setters and refs stay put.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    fx.onBusy(fx.busyText);
  }, [fx.busyText, fx.onBusy]);

  // Cho phép cửa sổ phát đúng file kết quả vừa xong, để xem ngay trong app.
  useEffect(() => {
    s.setPlayable(false);
    s.setCheckedMedia("");
    s.setOpenNote("");
    if (!fx.shown) return;
    let live = true;
    const key = watchMediaKey(fx.shown, s.resultEpoch);
    invoke("allow_result", { path: fx.shown })
      .then(() => {
        if (!live) return;
        s.setCheckedMedia(key);
        s.setPlayable(true);
      })
      .catch((caught) => {
        if (!live) return;
        s.setCheckedMedia(key);
        s.setOpenNote(errorText(caught));
      });
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [fx.shown, s.resultEpoch]);

  useEffect(() => {
    invoke<Prefs>("get_prefs")
      .then((prefs) => fx.applyPrefs(prefs, true))
      .catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [fx.prefsEpoch]);

  useEffect(() => {
    if (s.view !== "settings") return;
    const refresh = () => {
      invoke<Prefs>("get_prefs")
        .then((prefs) => fx.applyPrefs(prefs, true))
        .catch(() => {});
    };
    window.addEventListener("focus", refresh);
    return () => window.removeEventListener("focus", refresh);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [s.view]);

  useEffect(() => {
    if (!s.folder) {
      s.setDubs([]);
      return;
    }
    let live = true;
    invoke<FinishedDub[]>("list_dubs", { outputDir: s.folder })
      .then((items) => live && s.setDubs(items))
      .catch(() => live && s.setDubs([]));
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [s.folder, s.result, s.running, s.listTick]);

  useEffect(() => {
    if (s.view !== "watch" || !s.result) {
      s.setLines([]);
      s.setKept([]);
      s.setLoadedWatchData("");
      return;
    }
    let live = true;
    const key = watchDataKey(s.result, s.listenFile, s.resultEpoch);
    // Publish together: the script, warning card and version chips are one view.
    // Keep already loaded content visible during a background list refresh.
    void Promise.allSettled([
      invoke<ScriptLine[]>("dub_lines", { path: s.listenFile || s.result }),
      invoke<KeptVersion[]>("list_kept", { result: s.result }),
    ]).then(([lines, kept]) => {
      if (!live) return;
      s.setLines(lines.status === "fulfilled" ? lines.value : []);
      s.setKept(kept.status === "fulfilled" ? kept.value : []);
      s.setLoadedWatchData(key);
    });
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [s.view, s.result, s.resultEpoch, s.listTick, s.listenFile]);

  useEffect(() => {
    s.setPlayingKept("");
    s.setListenFile("");
    s.setAgainOpen(false);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [s.result]);

  useEffect(() => {
    s.setPicks({});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [s.result, s.resultEpoch]);

  useEffect(() => {
    let current = true;
    s.setVoices([]);
    s.setVoice("");
    s.setVoiceNote("");
    Promise.all([invoke<string[]>("list_voices",{lang:s.target}),invoke<string>("get_voice",{lang:s.target}).catch(()=>"")])
      .then(([names,name])=>{
        if(!current)return;
        s.setVoices(names);
        const wanted=s.pendingVoice.current?.lang===s.target?s.pendingVoice.current.name:"";
        s.setVoice(wanted&&names.includes(wanted)?wanted:name);
        if(wanted)s.pendingVoice.current=null;
      }).catch(caught=>current&&s.setVoiceNote(errorText(caught)));
    return () => {
      current = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [s.target]);

  useEffect(() => {
    if (!s.againOpen) return;
    let current = true;
    s.setAgainVoices([]);
    s.setAgainVoice("");
    s.setAgainSavedVoice("");
    invoke<string[]>("list_voices", { lang: s.againLang })
      .then((names) => current && s.setAgainVoices(names))
      .catch(() => current && s.setAgainVoices([]));
    invoke<string>("get_voice", { lang: s.againLang })
      .then((name) => {
        if (!current) return;
        s.setAgainVoice(name);
        s.setAgainSavedVoice(name);
      })
      .catch(() => {});
    return () => {
      current = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [s.againOpen, s.againLang]);

  useEffect(() => {
    void fx.loadChoice();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (!s.running) return;
    const timer = window.setInterval(() => s.setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [s.running]);

  useEffect(() => {
    const unlisten = listen<Progress>("dub-progress", (event) => {
      const next = event.payload;
      const stamp = Date.now();
      s.setNow(stamp);
      s.setSteps((current) =>
        current.map((step) =>
          step.id === next.id
            ? {
                ...step,
                label: next.label || step.label,
                state: next.state,
                detail: next.detail,
                // A message without a fraction keeps the last known one while the step is still running.
                progress: next.state === "running" ? (next.progress ?? step.progress) : null,
                startedAt: next.state === "running" ? (step.startedAt ?? stamp) : step.startedAt,
              }
            : step,
        ),
      );
    });
    return () => {
      void unlisten.then((stop) => stop());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    const unlisten = listen<{ detail: string; progress: number | null }>("download-progress", (event) => {
      s.setDownload(event.payload);
    });
    return () => {
      void unlisten.then((stop) => stop());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    const unlisten = listen<RunPause>("dub-pause", (event) => s.setRunPause(event.payload));
    return () => { void unlisten.then((stop) => stop()); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    const unlisten = listen<NamesPending>("names-review", (event) => {
      s.setNameEdit(false);
      s.setNameReview(event.payload);
      s.setNameRows(event.payload.names.length ? event.payload.names : [{ from: "", to: "" }]);
      s.setNameNote("");
      s.setView("progress");
    });
    return () => {
      void unlisten.then((stop) => stop());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    const saved = readSavedQueue();
    if (!saved.current) return;
    let live = true;
    const current = saved.current;
    invoke<DubStatus>("dub_status")
      .then((status) => {
        if (!live) return;
        const key = queueKey(current.mode, current.source);
        if (status.running) {
          s.activeJob.current = current;
          fx.rememberQueue(s.queueRef.current.filter((item) => queueKey(item.mode, item.source) !== key));
          return;
        }
        s.activeJob.current = null;
        writeSavedQueue({ waiting: s.queueRef.current, current: null });
      })
      .catch(() => {});
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    const unlisten = listen<WatchUpdate>("watch-update", (event) => {
      const update = event.payload;
      s.watchUpdateRef.current = update;
      s.setWatchUpdate(update);
      const slot = s.previewSlot.current;
      if (update.running && !slot.open && slot.epoch === 0) {
        slot.open = true;
        slot.listen = update.video === "" || isAudio(update.video);
      }
      if (!slot.open) return;
      const back = previewStillOpen(update);
      slot.open = back;
      if (!back) slot.epoch += 1;
      s.setReturnPreview(back);
      s.setReturnListen(slot.listen);
    });
    return () => {
      void unlisten.then((stop) => stop());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    const unlisten = listen<number>("listen-progress", (event) => {
      s.setJoinProgress(event.payload);
    });
    return () => {
      void unlisten.then((stop) => stop());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    const unlisten = listen<{ detail: string; progress: number | null }>("caption-progress", (event) => {
      s.setCaptionProgress(event.payload.progress);
    });
    return () => {
      void unlisten.then((stop) => stop());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
}
