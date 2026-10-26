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
};

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
};

export type ScriptLine = {
  start: number;
  end: number;
  source: string;
  translation: string;
  fit?: { cut_seconds: number; speed: number | null };
};

export type LinePick = {
  text: string;
  again: boolean;
};

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
};

export type Prefs = {
  outputDir: string;
  useTerms: boolean;
  burnCaptions: boolean;
  skipTranslate: boolean;
};
