import { useEffect, useRef, useState, type RefObject } from "react";
import "./PlaybackSpeed.css";

const SPEEDS = [0.5, 0.75, 1, 1.25, 1.5, 1.75, 2];
const number = new Intl.NumberFormat("vi-VN", { maximumFractionDigits: 2 });
const speedLabel = (rate: number) => `${number.format(rate)}×`;

// The WebView's native speed popup can close on subtitle DOM updates.
// Keep this menu in the app so timeupdate cannot dismiss it.
export default function PlaybackSpeed({ mediaRef }: { mediaRef: RefObject<HTMLMediaElement | null> }) {
  const [open, setOpen] = useState(false);
  const [rate, setRate] = useState(1);
  const root = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const options = useRef<Array<HTMLButtonElement | null>>([]);

  useEffect(() => {
    const media = mediaRef.current;
    if (!media) return;
    const sync = () => setRate(media.playbackRate);
    sync();
    media.addEventListener("ratechange", sync);
    return () => media.removeEventListener("ratechange", sync);
  }, [mediaRef]);

  useEffect(() => {
    if (!open) return;
    const dismiss = (event: PointerEvent) => {
      if (event.target instanceof Node && !root.current?.contains(event.target)) setOpen(false);
    };
    document.addEventListener("pointerdown", dismiss, true);
    const index = SPEEDS.indexOf(mediaRef.current?.playbackRate ?? 1);
    options.current[index < 0 ? 2 : index]?.focus();
    return () => document.removeEventListener("pointerdown", dismiss, true);
  }, [open, mediaRef]);

  function choose(value: number) {
    const media = mediaRef.current;
    if (!media) return;
    media.defaultPlaybackRate = value;
    media.playbackRate = value;
    setRate(value);
    setOpen(false);
    trigger.current?.focus();
  }

  return (
    <div className="playback-speed" ref={root}
      onBlur={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setOpen(false);
      }}
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.preventDefault();
          event.stopPropagation();
          setOpen(false);
          trigger.current?.focus();
        }
      }}>
      <button type="button" className="playback-speed-trigger" ref={trigger}
        aria-haspopup="menu" aria-expanded={open} onClick={() => setOpen(!open)}
        onKeyDown={(event) => {
          if (event.key === "ArrowDown" || event.key === "ArrowUp") {
            event.preventDefault();
            setOpen(true);
          }
        }}>
        Tốc độ · {speedLabel(rate)}
      </button>
      {open ? (
        <div className="playback-speed-menu" role="menu" aria-label="Tốc độ phát">
          {SPEEDS.map((value, index) => (
            <button type="button" role="menuitemradio" aria-checked={rate === value} key={value} tabIndex={-1}
              ref={(node) => { options.current[index] = node; }}
              onClick={() => choose(value)}
              onKeyDown={(event) => {
                let next: number;
                if (event.key === "ArrowRight" || event.key === "ArrowDown") next = (index + 1) % SPEEDS.length;
                else if (event.key === "ArrowLeft" || event.key === "ArrowUp") next = (index + SPEEDS.length - 1) % SPEEDS.length;
                else if (event.key === "Home") next = 0;
                else if (event.key === "End") next = SPEEDS.length - 1;
                else return;
                event.preventDefault();
                options.current[next]?.focus();
              }}>
              {speedLabel(value)}
            </button>
          ))}
        </div>
      ) : null}
    </div>
  );
}
