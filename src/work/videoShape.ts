// Browser metadata includes the displayed orientation after rotation.
export function markVideoShape(video: HTMLVideoElement) {
  if (video.videoWidth > 0 && video.videoHeight > 0) {
    video.dataset.shape = video.videoHeight > video.videoWidth ? "portrait" : "landscape";
    if (video.parentElement?.classList.contains("video-surface")) {
      video.parentElement.style.setProperty("--video-aspect", String(video.videoWidth / video.videoHeight));
      video.closest<HTMLElement>(".player-slot")?.style.setProperty("--video-aspect", String(video.videoWidth / video.videoHeight));
    }
    if (video.parentElement?.classList.contains("live-frame")) {
      video.parentElement.style.setProperty("--video-ratio", `${video.videoWidth} / ${video.videoHeight}`);
    }
  }
}
