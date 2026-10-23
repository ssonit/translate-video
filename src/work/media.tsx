import { memo, useCallback, type RefObject } from "react";

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
  const bind = useCallback(
    (node: HTMLMediaElement | null) => {
      mediaRef.current = node;
    },
    [mediaRef],
  );
  if (kind === "audio") {
    return <audio ref={bind} className="result-audio" src={src} controls preload="metadata" />;
  }
  return <video ref={bind} className="result-video" src={src} controls preload="metadata" />;
});
