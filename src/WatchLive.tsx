import { useCallback, useEffect, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import {
  advanceClock,
  audioMove,
  frame,
  listenErrorDetail,
  playerErrorDetail,
  voiceGate,
  wantsPlay,
  type Span,
} from "./watchPlay";

export type WatchLine = {
  index: number;
  start: number;
  end: number;
  source: string;
  translation: string;
  wav: string;
};

export type WatchUpdate = {
  video: string;
  readyUntil: number;
  running: boolean;
  published: boolean;
  detail: string;
  lines: WatchLine[];
  textOnly?: boolean;
};

type Props = {
  update: WatchUpdate;
  listen: boolean;
  onStop: () => void;
  onExport: () => void;
};

function spansOf(lines: WatchLine[]): Span[] {
  return lines.map((line) =>
    line.translation.trim() ? { start: line.start, end: line.end } : { start: line.end, end: line.end },
  );
}

function statusText(update: WatchUpdate, hold: boolean): string {
  if (update.readyUntil === 0) return "Đang chép đoạn đầu";
  if (hold) return "Đang dịch tiếp";
  return update.detail;
}

export default function WatchLive({ update, listen, onStop, onExport }: Props) {
  const mediaRef = useRef<HTMLMediaElement>(null);
  const bindMedia = useCallback((node: HTMLMediaElement | null) => {
    mediaRef.current = node;
  }, []);
  const audioRef = useRef<HTMLAudioElement>(null);
  const holding = useRef(false);
  const userPaused = useRef(false);
  const started = useRef(false);
  const wants = useRef(false);
  const lineIndex = useRef<number | null>(null);
  const wavPath = useRef("");
  const blockedRef = useRef(false);
  const clock = useRef(0);
  const clockPlaying = useRef(false);
  const finishing = useRef(false);
  const finishingLine = useRef<number | null>(null);
  const spentLine = useRef<number | null>(null);
  const seenTime = useRef(0);
  const holdingVoice = useRef(false);
  const updateRef = useRef(update);
  updateRef.current = update;
  const listenRef = useRef(listen);
  listenRef.current = listen;
  const [caption, setCaption] = useState<string | null>(null);
  const [current, setCurrent] = useState<number | null>(null);
  const [blocked, setBlocked] = useState(false);
  const [status, setStatus] = useState(() => statusText(update, false));
  const [playbackPaused, setPlaybackPaused] = useState(false);
  const listRef = useRef<HTMLOListElement>(null);
  const rowRef = useRef<HTMLLIElement>(null);

  function headlessNow() {
    return listenRef.current && updateRef.current.video === "";
  }

  function onMediaPause() {
    if (holding.current || holdingVoice.current) return;
    userPaused.current = true;
    wants.current = false;
    audioRef.current?.pause();
  }

  function onMediaPlay() {
    if (blockedRef.current) return;
    userPaused.current = false;
    if (updateRef.current.readyUntil > 0) started.current = true;
  }

  function failPlayback() {
    blockedRef.current = true;
    setBlocked(true);
    userPaused.current = true;
    wants.current = false;
    clockPlaying.current = false;
    mediaRef.current?.pause();
    setStatus(listenRef.current ? listenErrorDetail() : playerErrorDetail());
  }

  function apply() {
    const media = mediaRef.current;
    const audio = audioRef.current;
    const current = updateRef.current;
    const headless = headlessNow();
    if (!audio || blockedRef.current) return;
    if (!headless && !media) return;
    if (current.readyUntil > 0) started.current = true;
    const now = headless ? clock.current : media!.currentTime;
    const textOnly = current.textOnly === true;
    const shot = frame(now, current.readyUntil, current.running, spansOf(current.lines), !textOnly);
    if (headless) {
      if (Math.abs(shot.time - clock.current) > 0.05) clock.current = shot.time;
    } else if (Math.abs(shot.time - media!.currentTime) > 0.05) {
      media!.currentTime = shot.time;
    }
    if (!headless) media!.volume = shot.volume;
    audio.volume = 1;
    if (shot.hold) {
      holding.current = true;
      finishing.current = false;
      holdingVoice.current = false;
      if (!headless) media!.pause();
      clockPlaying.current = false;
    }
    const spokenLine = shot.line === null ? null : current.lines[shot.line];
    if (Math.abs(shot.time - seenTime.current) > 0.4) {
      finishing.current = false;
      holdingVoice.current = false;
      spentLine.current = null;
    }
    seenTime.current = shot.time;
    if (finishing.current && shot.line !== finishingLine.current) {
      finishing.current = false;
      holdingVoice.current = false;
    }
    const stillFinishing = finishing.current && !shot.hold;
    const gate =
      textOnly || !spokenLine || userPaused.current || shot.hold
        ? "play"
        : voiceGate(shot.time, spokenLine.end, {
            currentTime: audio.currentTime,
            duration: audio.duration,
            ended: audio.ended,
          }, stillFinishing);
    if (gate === "finish") {
      finishing.current = true;
      finishingLine.current = shot.line;
      holdingVoice.current = true;
      wants.current = false;
      clockPlaying.current = false;
      if (!headless && !media!.paused) media!.pause();
    }
    const should = wantsPlay(shot.hold || gate === "finish", userPaused.current, started.current);
    if (headless) {
      const finished = !current.running && clock.current >= current.readyUntil && current.readyUntil > 0;
      clockPlaying.current = should && !finished && gate !== "finish";
    } else if (!wants.current && should && gate !== "finish") {
      void media!.play();
    }
    wants.current = should && gate !== "finish";
    if (!shot.hold) holding.current = false;

    const nextStatus = statusText(current, shot.hold);
    setStatus((prev) => (prev === nextStatus ? prev : nextStatus));
    const nextCaption = shot.line === null ? null : (current.lines[shot.line]?.translation ?? null);
    setCaption((prev) => (prev === nextCaption ? prev : nextCaption));
    setCurrent((prev) => (prev === shot.line ? prev : shot.line));

    if (userPaused.current || textOnly) {
      audio.pause();
      return;
    }
    if (gate === "finish") {
      if (audio.paused) void audio.play();
      return;
    }
    holdingVoice.current = false;
    if (gate === "done") {
      finishing.current = false;
      spentLine.current = shot.line;
      audio.pause();
    }
    if (shot.hold || shot.line === null) {
      audio.pause();
      if (shot.line === null) {
        lineIndex.current = null;
        wavPath.current = "";
      }
      return;
    }
    const line = current.lines[shot.line];
    if (spentLine.current === shot.line) {
      audio.pause();
      lineIndex.current = shot.line;
      return;
    }
    const changed = lineIndex.current !== shot.line || wavPath.current !== line.wav;
    const move = audioMove(shot.time, line.start, changed, {
      currentTime: audio.currentTime,
      duration: audio.duration,
      ended: audio.ended,
    });
    if (move === "stop") {
      audio.pause();
      lineIndex.current = shot.line;
      return;
    }
    if (move === "start" || move === "seek") {
      if (wavPath.current !== line.wav) {
        audio.src = convertFileSrc(line.wav);
        wavPath.current = line.wav;
      }
      audio.currentTime = Math.max(0, shot.time - line.start);
      lineIndex.current = shot.line;
      void audio.play();
      return;
    }
    if (audio.paused) void audio.play();
  }

  useEffect(() => {
    let frameId = 0;
    let previous = performance.now();
    const tick = (now: number) => {
      const delta = Math.min(0.25, (now - previous) / 1000);
      previous = now;
      const headless = listenRef.current && updateRef.current.video === "";
      const media = mediaRef.current;
      if (headless && clockPlaying.current && !finishing.current) {
        const current = updateRef.current;
        clock.current = advanceClock(clock.current, delta, current.readyUntil, current.running, true);
        apply();
      } else if (finishing.current || (media && !media.paused)) {
        apply();
      }
      frameId = requestAnimationFrame(tick);
    };
    frameId = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frameId);
  }, []);

  useEffect(() => {
    apply();
    // Re-apply when a new cluster arrives so the lead and the caption move without waiting for timeupdate.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [update]);

  useEffect(() => {
    const row = rowRef.current;
    const list = listRef.current;
    if (!row || !list) return;
    const top = row.offsetTop;
    const bottom = top + row.offsetHeight;
    if (top < list.scrollTop) list.scrollTop = top;
    else if (bottom > list.scrollTop + list.clientHeight) list.scrollTop = bottom - list.clientHeight;
  }, [current]);

  function togglePlayback() {
    const next = !userPaused.current;
    userPaused.current = next;
    if (next) {
      clockPlaying.current = false;
      audioRef.current?.pause();
    }
    setPlaybackPaused(next);
    apply();
  }

  const headless = listen && update.video === "";
  const showActions = update.running || update.published;

  return (
    <div className={listen ? "live is-listen" : "live"}>
      <div className="live-head">
        <strong>{listen ? "Nghe trước" : "Xem trước"}</strong>
        {blocked ? null : <p className="live-status">{status}</p>}
      </div>
      {blocked ? (
        <div className="live-missing">
          <strong>{listen ? "Chưa có tiếng" : "Chưa có hình"}</strong>
          <p>{listen ? listenErrorDetail() : playerErrorDetail()}</p>
        </div>
      ) : (
        <div className="live-stage">
          {headless ? null : listen ? (
            <audio
              ref={bindMedia}
              className="result-audio"
              controls
              src={convertFileSrc(update.video)}
              onTimeUpdate={apply}
              onPause={onMediaPause}
              onPlay={onMediaPlay}
              onError={failPlayback}
            />
          ) : (
            <div className="live-frame">
              <video
                ref={bindMedia}
                className="result-video"
                controls
                src={convertFileSrc(update.video)}
                onTimeUpdate={apply}
                onPause={onMediaPause}
                onPlay={onMediaPlay}
                onError={failPlayback}
              />
              {caption ? <p className="live-caption">{caption}</p> : null}
            </div>
          )}
          {listen ? (
            <p className={caption ? "live-caption is-listen" : "live-caption is-listen is-wait"}>
              {caption || "Chưa có câu"}
            </p>
          ) : null}
          {headless && update.readyUntil > 0 ? (
            <button type="button" className="check live-play" onClick={togglePlayback}>
              {playbackPaused ? "Nghe tiếp" : "Tạm dừng"}
            </button>
          ) : null}
        </div>
      )}
      <audio ref={audioRef} />
      <ol className="live-lines" ref={listRef}>
        {update.lines.map((line, index) => {
          const spoken = line.translation.trim();
          const source = line.source.trim();
          return (
            <li
              key={line.index}
              ref={index === current ? rowRef : undefined}
              className={index === current ? "is-current" : undefined}
            >
              <time>{stamp(line.start)}</time>
              <span>
                <strong>{spoken || source}</strong>
                {spoken && source && spoken !== source ? <small>{source}</small> : null}
              </span>
            </li>
          );
        })}
      </ol>
      {showActions ? (
        <div className="live-actions">
          {update.running ? (
            <button type="button" className={update.published ? "link" : "check"} onClick={onStop}>
              Dừng
            </button>
          ) : null}
          {update.published ? (
            <button type="button" className="check" onClick={onExport}>
              {listen ? "Xuất audio" : "Ghép video"}
            </button>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

function stamp(seconds: number) {
  const total = Math.max(0, Math.floor(seconds));
  return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, "0")}`;
}
