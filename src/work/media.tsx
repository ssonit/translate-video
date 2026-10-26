import { memo, useCallback, useEffect, useState, type RefObject } from "react";
import PlaybackSpeed from "./PlaybackSpeed";

// Phần tử phát đứng riêng: cửa sổ vẽ lại thì không gắn lại src, video không tải từ đầu.
export const ResultMedia = memo(function ResultMedia({
  kind,
  src,
  mediaRef,
}: {
  kind: "audio" | "video";
  src: string;
  mediaRef: RefObject<HTMLMediaElement | null>;
}) {
  const [nativeFullscreen, setNativeFullscreen] = useState(false);
  useEffect(() => {
    // Native video fullscreen excludes sibling app controls. Keep its speed
    // option available there; the app menu handles normal window playback.
    const sync = () => setNativeFullscreen(document.fullscreenElement === mediaRef.current);
    document.addEventListener("fullscreenchange", sync);
    return () => document.removeEventListener("fullscreenchange", sync);
  }, [mediaRef]);
  const bind = useCallback(
    (node: HTMLMediaElement | null) => {
      mediaRef.current = node;
    },
    [mediaRef],
  );
  if (kind === "audio") {
    return <audio ref={bind} className="result-audio" src={src} controls preload="metadata" />;
  }
  return (
    <>
      <video ref={bind} className="result-video" src={src} controls controlsList={nativeFullscreen ? undefined : "noplaybackrate"} preload="metadata" />
      <PlaybackSpeed mediaRef={mediaRef} />
    </>
  );
});
