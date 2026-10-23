import { useRef, useState } from "react";
import { readSavedQueue, restoreQueue, type QueueJob } from "../queue";
import type { WatchUpdate } from "../WatchLive";
import { freshSteps, LANGUAGES } from "./format";
import type {
  FinishedDub,
  JobSpec,
  KeptVersion,
  LinePick,
  NameFix,
  NamesPending,
  ScriptLine,
  StepView,
  View,
} from "./types";

export function useWorkState() {
  const [target, setTarget] = useState(() => {
    try {
      const saved = localStorage.getItem("target-language");
      return LANGUAGES.some((item) => item.code === saved) ? (saved as string) : "vi";
    } catch {
      return "vi";
    }
  });
  const [voices, setVoices] = useState<string[]>([]);
  const [voice, setVoice] = useState("");
  const [voiceNote, setVoiceNote] = useState("");
  const [previewing, setPreviewing] = useState(false);
  const [mode, setMode] = useState<"file" | "link">("link");
  const [link, setLink] = useState("");
  const [composeVideo, setComposeVideo] = useState("");
  const [composeLink, setComposeLink] = useState("");
  const [queue, setQueue] = useState<QueueJob[]>(() => restoreQueue(readSavedQueue(), false));
  const [queueNote, setQueueNote] = useState("");
  const [downloading, setDownloading] = useState(false);
  const [download, setDownload] = useState<{ detail: string; progress: number | null }>({
    detail: "",
    progress: null,
  });
  const [video, setVideo] = useState("");
  const [folder, setFolder] = useState("");
  const [useTerms, setUseTerms] = useState(false);
  const [burnCaptions, setBurnCaptions] = useState(false);
  const [skipTranslate, setSkipTranslate] = useState(false);
  const [audioOnly, setAudioOnly] = useState(false);
  const [textOnly, setTextOnly] = useState(false);
  const [partsRoot, setPartsRoot] = useState("");
  const [savingAudio, setSavingAudio] = useState(false);
  const [joining, setJoining] = useState(false);
  const [joinProgress, setJoinProgress] = useState<number | null>(null);
  const [listenFile, setListenFile] = useState("");
  const [prefsNote, setPrefsNote] = useState("");
  const [steps, setSteps] = useState<StepView[]>(freshSteps);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState("");
  const [result, setResult] = useState("");
  const [resultTextOnly, setResultTextOnly] = useState(false);
  const [runStartedAt, setRunStartedAt] = useState<number | null>(null);
  const [now, setNow] = useState(() => Date.now());
  const [provider, setProvider] = useState("local");
  const [model, setModel] = useState("");
  const [modelNote, setModelNote] = useState("");
  const [copied, setCopied] = useState("");
  const [customModel, setCustomModel] = useState(false);
  const [pending, setPending] = useState("");
  const [keyDraft, setKeyDraft] = useState("");
  const [playable, setPlayable] = useState(false);
  const [captioning, setCaptioning] = useState(false);
  const [captionProgress, setCaptionProgress] = useState<number | null>(null);
  const [captionNote, setCaptionNote] = useState("");
  const [openNote, setOpenNote] = useState("");
  const [dubs, setDubs] = useState<FinishedDub[]>([]);
  const [listQuery, setListQuery] = useState("");
  const [removing, setRemoving] = useState("");
  const [listNote, setListNote] = useState("");
  const [listTick, setListTick] = useState(0);
  const [lines, setLines] = useState<ScriptLine[]>([]);
  const [picks, setPicks] = useState<Record<number, LinePick>>({});
  const [resultEpoch, setResultEpoch] = useState(0);
  const [view, setView] = useState<View>("new");
  const viewRef = useRef<View>("new");
  viewRef.current = view;
  const [watchUpdate, setWatchUpdate] = useState<WatchUpdate | null>(null);
  const watchUpdateRef = useRef<WatchUpdate | null>(null);
  const previewSlot = useRef({ open: false, listen: false, epoch: 0 });
  const [returnPreview, setReturnPreview] = useState(false);
  const [returnListen, setReturnListen] = useState(false);
  const [draft, setDraft] = useState<FinishedDub | null>(null);
  const [nameReview, setNameReview] = useState<NamesPending | null>(null);
  const [nameEdit, setNameEdit] = useState(false);
  const [nameRoot, setNameRoot] = useState("");
  const [nameRows, setNameRows] = useState<NameFix[]>([]);
  const [nameNote, setNameNote] = useState("");
  const [nameSending, setNameSending] = useState(false);
  const [kept, setKept] = useState<KeptVersion[]>([]);
  const [playingKept, setPlayingKept] = useState("");
  const [againOpen, setAgainOpen] = useState(false);
  const [againNames, setAgainNames] = useState<NameFix[]>([]);
  const [againBase, setAgainBase] = useState<NameFix[]>([]);
  const [againLang, setAgainLang] = useState("vi");
  const [againVoice, setAgainVoice] = useState("");
  const [againSavedVoice, setAgainSavedVoice] = useState("");
  const [againVoices, setAgainVoices] = useState<string[]>([]);
  const [retranscribe, setRetranscribe] = useState(false);
  const [againTextOnly, setAgainTextOnly] = useState(false);
  const [againTextBase, setAgainTextBase] = useState(false);
  const [againNote, setAgainNote] = useState("");
  // True while run() in this window awaits the backend; otherwise a running job belongs to a window that was reloaded.
  const ownsRun = useRef(false);
  const queueRef = useRef<QueueJob[]>(queue);
  const activeJob = useRef<QueueJob | null>(null);
  const holdQueue = useRef(false);
  const gate = useRef(false);
  const retryRef = useRef<JobSpec | null>(null);
  const mediaRef = useRef<HTMLMediaElement | null>(null);

  return {
    target,
    setTarget,
    voices,
    setVoices,
    voice,
    setVoice,
    voiceNote,
    setVoiceNote,
    previewing,
    setPreviewing,
    mode,
    setMode,
    link,
    setLink,
    composeVideo,
    setComposeVideo,
    composeLink,
    setComposeLink,
    queue,
    setQueue,
    queueNote,
    setQueueNote,
    downloading,
    setDownloading,
    download,
    setDownload,
    video,
    setVideo,
    folder,
    setFolder,
    useTerms,
    setUseTerms,
    burnCaptions,
    setBurnCaptions,
    skipTranslate,
    setSkipTranslate,
    audioOnly,
    setAudioOnly,
    textOnly,
    setTextOnly,
    partsRoot,
    setPartsRoot,
    savingAudio,
    setSavingAudio,
    joining,
    setJoining,
    joinProgress,
    setJoinProgress,
    listenFile,
    setListenFile,
    prefsNote,
    setPrefsNote,
    steps,
    setSteps,
    running,
    setRunning,
    error,
    setError,
    result,
    setResult,
    resultTextOnly,
    setResultTextOnly,
    runStartedAt,
    setRunStartedAt,
    now,
    setNow,
    provider,
    setProvider,
    model,
    setModel,
    modelNote,
    setModelNote,
    copied,
    setCopied,
    customModel,
    setCustomModel,
    pending,
    setPending,
    keyDraft,
    setKeyDraft,
    playable,
    setPlayable,
    captioning,
    setCaptioning,
    captionProgress,
    setCaptionProgress,
    captionNote,
    setCaptionNote,
    openNote,
    setOpenNote,
    dubs,
    setDubs,
    listQuery,
    setListQuery,
    removing,
    setRemoving,
    listNote,
    setListNote,
    listTick,
    setListTick,
    lines,
    setLines,
    picks,
    setPicks,
    resultEpoch,
    setResultEpoch,
    view,
    setView,
    viewRef,
    watchUpdate,
    setWatchUpdate,
    watchUpdateRef,
    previewSlot,
    returnPreview,
    setReturnPreview,
    returnListen,
    setReturnListen,
    draft,
    setDraft,
    nameReview,
    setNameReview,
    nameEdit,
    setNameEdit,
    nameRoot,
    setNameRoot,
    nameRows,
    setNameRows,
    nameNote,
    setNameNote,
    nameSending,
    setNameSending,
    kept,
    setKept,
    playingKept,
    setPlayingKept,
    againOpen,
    setAgainOpen,
    againNames,
    setAgainNames,
    againBase,
    setAgainBase,
    againLang,
    setAgainLang,
    againVoice,
    setAgainVoice,
    againSavedVoice,
    setAgainSavedVoice,
    againVoices,
    setAgainVoices,
    retranscribe,
    setRetranscribe,
    againTextOnly,
    setAgainTextOnly,
    againTextBase,
    setAgainTextBase,
    againNote,
    setAgainNote,
    ownsRun,
    queueRef,
    activeJob,
    holdQueue,
    gate,
    retryRef,
    mediaRef,
  };
}

export type Session = ReturnType<typeof useWorkState>;
