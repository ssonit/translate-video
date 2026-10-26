import { useEffect, useRef, useState, type RefObject } from "react";
import "./VideoMenu.css";

export default function VideoMenu({ mediaRef, onDownload, saving = false }: {
  mediaRef: RefObject<HTMLMediaElement | null>;
  onDownload?: () => void;
  saving?: boolean;
}) {
  const [open, setOpen] = useState(false);
  const [error, setError] = useState("");
  const root = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!open) return;
    const outside = (event: PointerEvent) => {
      if (event.target instanceof Node && !root.current?.contains(event.target)) setOpen(false);
    };
    document.addEventListener("pointerdown", outside, true);
    root.current?.querySelector<HTMLButtonElement>('[role="menuitem"]:not(:disabled)')?.focus();
    return () => document.removeEventListener("pointerdown", outside, true);
  }, [open]);

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
    <div className="video-menu" ref={root}
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
        if (event.key === "ArrowDown" || event.key === "ArrowUp" || event.key === "Home" || event.key === "End") {
          event.preventDefault();
          if (!open) { setOpen(true); return; }
          const buttons = Array.from(root.current?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not(:disabled)') ?? []);
          const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
          const next = event.key === "Home" ? 0 : event.key === "End" ? buttons.length - 1
            : (index + (event.key === "ArrowUp" ? buttons.length - 1 : 1)) % buttons.length;
          buttons[next]?.focus();
        }
      }}>
      <button type="button" className="video-menu-trigger" ref={trigger} aria-label="Tùy chọn video"
        aria-haspopup="menu" aria-expanded={open} onClick={() => { setError(""); setOpen(!open); }}>
        <span aria-hidden="true">⋮</span>
      </button>
      {open ? (
        <div className="video-menu-popup" role="menu" aria-label="Tùy chọn video">
          <button type="button" role="menuitem" disabled={!onDownload || saving} onClick={() => {
            setOpen(false);
            trigger.current?.focus();
            onDownload?.();
          }}>{saving ? "Đang lưu video…" : "Tải video"}</button>
          <button type="button" role="menuitem" disabled={!document.pictureInPictureEnabled} onClick={() => void pictureInPicture()}>
            Hình trong hình
          </button>
          {error ? <p role="alert">{error}</p> : null}
        </div>
      ) : null}
    </div>
  );
}
