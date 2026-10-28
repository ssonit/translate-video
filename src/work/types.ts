export type StepView = {
  id: string;
  label: string;
  weight: number;
  state: string;
  detail: string;
  progress: number | null;
  startedAt: number | null;
};

export type Progress = {
  id: string;
  label: string;
  state: string;
  detail: string;
  progress: number | null;
};

// What the backend remembers of the current or last run (command dub_status, event dub-finished).
export type DubStatus = {
  running: boolean;
  downloading: boolean;
  startedMs: number;
  video: string;
  outputDir: string;
  download: { detail: string; progress: number | null } | null;
  steps: Progress[];
  result: string | null;
  error: string | null;
  audioOnly?: boolean;
  textOnly?: boolean;
  names?: NamesPending | null;
  pause?: RunPause;
};

export type RunPause = { pausedMs: number; sinceMs: number | null };

export type NameFix = { from: string; to: string };

export type NamesPending = { names: NameFix[]; note: string };

export type NameList = { confirmed: boolean; names: NameFix[] };

export type FinishedDub = {
  title: string;
  path: string;
  language: string;
  modifiedMs: number;
  done: boolean;
  video: string;
  captioned: boolean;
  audio: boolean;
  url?: string;
};

// Vài con số và câu đầu của một việc dở, để thẻ việc dở cho người dùng nhớ đó là video nào (lệnh draft_brief).
export type DraftBrief = {
  heard: number;
  translated: number;
  spoken: number;
  textOnly: boolean;
  excerpt: string[];
  excerptTranslated: boolean;
};

export type KeptVersion = {
  path: string;
  language: string;
  modifiedMs: number;
  audio: boolean;
  versionId?: string | null;
  summary?: string | null;
  knownConfig?: boolean;
};

export type VersionConfig = {
  target: string; voice: string; useTerms: boolean; skipTranslate: boolean; textOnly: boolean; audioOnly: boolean; burnCaptions: boolean;
  render: import("./renderSettings").RenderSettings; translation: { provider: string; model: string } | null;
};
export type VersionDetails = { id: string | null; createdMs: number; reasons: string[]; config: VersionConfig | null; lines: number | null;
  files: { path: string; kind: "audio" | "video" | "captioned" }[]; note: string };

export type ScriptLine = {
  start: number;
  end: number;
  source: string;
  translation: string;
  fit?: { cut_seconds: number; speed: number | null };
  missing_audio?: boolean;
  revision?: string;
  id?: string;
  sources?: number[];
  structure_undo?: boolean;
  structure_undo_blocked?: boolean;
  captions?: ("invalid_timing" | "overlap" | "short_duration" | "dense_text")[];
};

export type LinePick = {
  text: string;
  again: boolean;
  start?: number;
  end?: number;
  revision?: string;
};

export type LineEdit = { index: number; text: string; start?: number; end?: number; revision?: string; again?: boolean; id?: string };
export type StructureEdit =
  | { kind: "split"; revision: string; id: string; at: number; left_source: string; left_text: string; right_source: string; right_text: string }
  | { kind: "merge"; revision: string; id: string; next_id: string; source: string; text: string }
  | { kind: "undo"; revision: string };

export type View = "new" | "progress" | "watch" | "live" | "settings" | "resume" | "parts";

export type JobSpec = {
  mode: "file" | "link";
  source: string;
  folder: string;
  target: string;
  voice: string;
  audioOnly: boolean;
  skipTranslate: boolean;
  burnCaptions: boolean;
  textOnly: boolean;
  render?: import("./renderSettings").RenderSettings;
  translation?: import("../queue").TranslationRun;
};

export type Prefs = {
  outputDir: string;
  useTerms: boolean;
  burnCaptions: boolean;
  skipTranslate: boolean;
};
