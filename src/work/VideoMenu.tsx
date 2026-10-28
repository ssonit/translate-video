import { useEffect, useRef, useState, type RefObject } from "react";
import "./VideoMenu.css";

const SPEEDS = [0.5, 0.75, 1, 1.25, 1.5, 1.75, 2];
const number = new Intl.NumberFormat("vi-VN", { maximumFractionDigits: 2 });
const speedLabel = (rate: number) => `${number.format(rate)}×`;

export default function VideoMenu({ mediaRef, onDownload, saving = false }: {
  mediaRef: RefObject<HTMLMediaElement | null>;
  onDownload?: () => void;
  saving?: boolean;
}) {
  const [open, setOpen] = useState(false);
  const [error, setError] = useState("");
  const [page, setPage] = useState<"main" | "speed">("main");
  const [rate, setRate] = useState(1);
  const [paused, setPaused] = useState(true);
  const root = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    const media = mediaRef.current;
    if (!media) return;
    const sync = () => { setRate(media.playbackRate); setPaused(media.paused); };
    sync();
    for (const name of ["ratechange", "play", "pause", "ended"]) media.addEventListener(name, sync);
    return () => {
      for (const name of ["ratechange", "play", "pause", "ended"]) media.removeEventListener(name, sync);
    };
  }, [mediaRef]);

  useEffect(() => {
    if (!open) return;
    const outside = (event: PointerEvent) => {
      if (event.target instanceof Node && !root.current?.contains(event.target)) setOpen(false);
    };
    document.addEventListener("pointerdown", outside, true);
    const target = root.current?.querySelector<HTMLButtonElement>(page === "speed"
      ? '[role="menuitemradio"][aria-checked="true"]' : '[role="menuitem"]:not(:disabled)')
      ?? root.current?.querySelector<HTMLButtonElement>('[role="menuitemradio"]');
    target?.focus();
    return () => document.removeEventListener("pointerdown", outside, true);
  }, [open, page]);

  function chooseSpeed(value: number) {
    const media = mediaRef.current;
    if (!media) return;
    media.defaultPlaybackRate = value;
    media.playbackRate = value;
    setRate(value);
    setOpen(false);
    trigger.current?.focus();
  }

  async function pictureInPicture() {
    const video = mediaRef.current;
    if (!(video instanceof HTMLVideoElement)) return;
    setError("");
    // Hide the duplicate native menu, but allow an explicit app action to
    // request PiP. Restore the attribute even if the WebView rejects it.
    const disabled = video.disablePictureInPicture;
    video.disablePictureInPicture = false;
    try {
      if (document.pictureInPictureElement === video) await document.exitPictureInPicture();
      else await video.requestPictureInPicture();
      setOpen(false);
      trigger.current?.focus();
    } catch {
      setError("Chưa mở được hình trong hình. Thử phát video rồi chọn lại.");
    } finally {
      video.disablePictureInPicture = disabled;
    }
  }

  return (
    <div className={["video-menu", paused ? "is-paused" : "", open ? "is-open" : ""].filter(Boolean).join(" ")} ref={root}
      onBlur={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setOpen(false);
      }}
      onKeyDown={(event) => {
        if (event.key === "ArrowLeft" && open && page === "speed") {
          event.preventDefault();
          setPage("main");
          return;
        }
        if (event.key === "Escape") {
          event.preventDefault();
          event.stopPropagation();
          setOpen(false);
          trigger.current?.focus();
        }
        if (event.key === "ArrowDown" || event.key === "ArrowUp" || event.key === "ArrowRight" || event.key === "Home" || event.key === "End") {
          event.preventDefault();
          if (!open) { setPage("main"); setOpen(true); return; }
          if (event.key === "ArrowRight" && page === "main") {
            if (document.activeElement?.getAttribute("aria-haspopup") === "menu") setPage("speed");
            return;
          }
          const buttons = Array.from(root.current?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not(:disabled), [role="menuitemradio"]') ?? []);
          if (buttons.length === 0) return;
          const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
          const next = event.key === "Home" ? 0 : event.key === "End" ? buttons.length - 1
            : (index + (event.key === "ArrowUp" ? buttons.length - 1 : 1)) % buttons.length;
          buttons[next]?.focus();
        }
      }}>
      <button type="button" className="video-menu-trigger" ref={trigger} aria-label="Tùy chọn video"
        aria-haspopup="menu" aria-expanded={open} onClick={() => { setError(""); setPage("main"); setOpen(!open); }}>
        <span aria-hidden="true">⋮</span>
      </button>
      {open ? (
        <div className="video-menu-popup" role="menu" aria-label={page === "speed" ? "Tốc độ phát" : "Tùy chọn video"}>
          {page === "speed" ? <>
            <button type="button" className="video-menu-back" onClick={() => setPage("main")} aria-label="Quay lại tùy chọn video">
              <span aria-hidden="true">‹</span> Tốc độ phát
            </button>
            <div className="video-menu-speeds">
              {SPEEDS.map((value) => <button key={value} type="button" role="menuitemradio" aria-checked={rate === value}
                onClick={() => chooseSpeed(value)}>{speedLabel(value)}</button>)}
            </div>
          </> : <>
          <button type="button" role="menuitem" aria-haspopup="menu" onClick={() => setPage("speed")}>
            <span>Tốc độ</span><span className="video-menu-value">{speedLabel(rate)} <span aria-hidden="true">›</span></span>
          </button>
          <button type="button" role="menuitem" disabled={!onDownload || saving} onClick={() => {
            setOpen(false);
            trigger.current?.focus();
            onDownload?.();
          }}>{saving ? "Đang lưu video…" : "Tải video"}</button>
          <button type="button" role="menuitem" disabled={!document.pictureInPictureEnabled} onClick={() => void pictureInPicture()}>
            Hình trong hình
          </button>
          </>}
          {error ? <p role="alert">{error}</p> : null}
        </div>
      ) : null}
    </div>
  );
}
