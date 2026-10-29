import { useEffect, useRef, useState } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";

// Authorize only visible local media; a missing source keeps its neutral placeholder.
export function LibraryPreview({ path, audio }: { path: string; audio: boolean }) {
  const host = useRef<HTMLSpanElement>(null);
  const [src, setSrc] = useState("");
  useEffect(() => {
    let live = true;
    setSrc("");
    if (!path || audio || !host.current) return;
    const observer = new IntersectionObserver(entries => {
      if (!entries.some(entry => entry.isIntersecting)) return;
      observer.disconnect();
      void invoke("allow_result", { path }).then(() => { if (live) setSrc(convertFileSrc(path)); }).catch(() => {});
    });
    observer.observe(host.current);
    return () => { live = false; observer.disconnect(); };
  }, [path, audio]);
  return <span className="library-preview" ref={host} aria-hidden="true">
    <span>{audio ? "♪" : "▷"}</span>
    {src ? <video src={src} muted playsInline preload="metadata" tabIndex={-1} onLoadedMetadata={event => { event.currentTarget.currentTime = Math.min(.2, event.currentTarget.duration / 2); }} onError={() => setSrc("")} /> : null}
  </span>;
}
