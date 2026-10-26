import { memo, useCallback, useEffect, useState, type RefObject } from "react";
import PlaybackSpeed from "./PlaybackSpeed";
import VideoMenu from "./VideoMenu";

// Phần tử phát đứng riêng: cửa sổ vẽ lại thì không gắn lại src, video không tải từ đầu.
export const ResultMedia = memo(function ResultMedia({
  kind,
  src,
  mediaRef,
  onDownload,
  savingVideo,
}: {
  kind: "audio" | "video";
  src: string;
  mediaRef: RefObject<HTMLMediaElement | null>;
  onDownload?: () => void;
  savingVideo?: boolean;
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
      <video ref={bind} className={nativeFullscreen ? "result-video" : "result-video has-app-menu"} src={src} controls
        controlsList={nativeFullscreen ? undefined : "nodownload noplaybackrate noremoteplayback"}
        disablePictureInPicture={!nativeFullscreen} preload="metadata" />
      <PlaybackSpeed mediaRef={mediaRef} />
      <VideoMenu mediaRef={mediaRef} onDownload={onDownload} saving={savingVideo} />
    </>
  );
});
