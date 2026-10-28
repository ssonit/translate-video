import { convertFileSrc } from "@tauri-apps/api/core";
import { useEffect, useState, type RefObject } from "react";
import { showListenStop } from "../listenStop";
import {
  captionedVideo,
  fileName,
  isAudio,
  isCaptioned,
  isListen,
  showAsTextOnly,
  watchHeading,
  languageName,
  parentFolder,
  plainVideo,
  sameFile,
  videoToSave,
} from "./format";
import { ResultMedia } from "./media";
import { DubScript } from "./script";
import { VersionHistory } from "./VersionHistory";
import type { FinishedDub, KeptVersion, LineEdit, LinePick, ScriptLine, StructureEdit } from "./types";

export function WatchPane({
  shown,
  result,
  playingKept,
  listenFile,
  lines,
  joining,
  joinProgress,
  playable,
  loading = false,
  resultEpoch,
  mediaRef,
  kept,
  watched,
  savingAudio,
  savingVideo,
  running,
  downloading,
  captioning,
  captionProgress,
  textReady,
  textOnlyRun,
  openNote,
  captionNote,
  error,
  picks,
  onPicks,
  onPlayCurrent,
  onPlayKept,
  onDeleteKept,
  onOpen,
  onFetchVideo,
  onDownloadAudio,
  onDownloadVideo,
  onJoin,
  onShowVideo,
  onStop,
  onAddCaptions,
  onAgain,
  onEditExport,
  onApplyVersion,
  onShowResult,
  onRetake,
  onStructure,
}: {
  shown: string;
  result: string;
  playingKept: string;
  listenFile: string;
  lines: ScriptLine[];
  joining: boolean;
  joinProgress: number | null;
  playable: boolean;
  loading?: boolean;
  resultEpoch: number;
  mediaRef: RefObject<HTMLMediaElement | null>;
  kept: KeptVersion[];
  watched: FinishedDub | undefined;
  savingAudio: boolean;
  savingVideo: boolean;
  running: boolean;
  downloading: boolean;
  captioning: boolean;
  captionProgress: number | null;
  textReady: boolean;
  textOnlyRun: boolean;
  openNote: string;
  captionNote: string;
  error: string;
  picks: Record<number, LinePick>;
  onPicks: (picks: Record<number, LinePick>) => void;
  onPlayCurrent: () => void;
  onPlayKept: (path: string) => void;
  onDeleteKept: (item: KeptVersion) => void;
  onOpen: (path: string) => void;
  onFetchVideo: () => void;
  onDownloadAudio: () => void;
  onDownloadVideo: () => void;
  onJoin: () => void;
  onShowVideo: () => void;
  onStop: () => void;
  onAddCaptions: () => void;
  onAgain: () => void;
  onEditExport: () => void;
  onApplyVersion: (path: string) => void;
  onShowResult: (path: string) => void;
  onRetake: (edits: LineEdit[]) => void;
  onStructure: (action: StructureEdit) => void;
}) {
  const [metadataReady, setMetadataReady] = useState(false);
  const [mediaNote, setMediaNote] = useState("");
  useEffect(() => {
    // Retake releases the old media handle before replacing its file. An error
    // from that released element must not remain on the newly loaded export.
    setMediaNote("");
    setMetadataReady((mediaRef.current?.readyState ?? 0) >= 1);
  }, [shown, resultEpoch, mediaRef]);
  const waiting = loading || (playable && !isAudio(shown) && !metadataReady);
  const mediaReady = () => setMetadataReady(true);
  const mediaFailed = () => {
    setMetadataReady(true);
    setMediaNote(isAudio(shown) ? "Không đọc được tiếng. Thử mở bằng trình phát." : "Không đọc được hình video. Thử mở bằng trình phát.");
  };
  const src = resultEpoch > 0 ? `${convertFileSrc(shown)}?v=${resultEpoch}` : convertFileSrc(shown);
  const textOnly = showAsTextOnly(result, watched?.path ?? "", textOnlyRun);
  // Lưu đúng video đang xem, nên có cả ở bản đã cất. Kết quả chỉ audio thì không có nút này.
  const saveVideo = videoToSave(result, playingKept) ? (
    <button type="button" className="check" onClick={() => void onDownloadVideo()} disabled={savingVideo}>
      {savingVideo ? "Đang lưu video…" : "Tải video"}
    </button>
  ) : null;
  return (
    <div
      className={["pane-fill watch", waiting ? "is-loading" : "", !playingKept && lines.length > 0 ? "has-script" : "", isAudio(shown) ? "is-audio" : "", joining ? "is-joining" : ""]
        .filter(Boolean)
        .join(" ")}
      aria-busy={waiting}
    >
      {waiting ? <div className="watch-loading" role="status">
        <span className="watch-loading-dot" aria-hidden="true" />
        <p>{isAudio(shown) ? "Đang mở audio…" : "Đang mở video…"}</p>
      </div> : null}
      <div className="result-head">
        <strong>
          {playingKept
            ? "Bản đã cất"
            : listenFile
              ? "Audio liền mạch"
              : isAudio(result)
                ? "Audio bản dịch"
                : watchHeading(result, watched?.path ?? "", textOnlyRun)}
        </strong>
        <span title={shown}>{watched?.title || fileName(isAudio(shown) ? shown : plainVideo(shown))}</span>
      </div>
      <div className="versions">
        <div className="version-row">
          <button type="button" className={playingKept ? "link" : "link is-on"} onClick={onPlayCurrent} disabled={running || captioning || joining}>
            Đang dùng{watched?.language ? ` · ${languageName(watched.language)}` : ""}
          </button>
        </div>
        {!playingKept ? <button type="button" className="link" title="Âm thanh, cách đọc, logo và chữ" disabled={running || captioning || joining} onClick={onEditExport}>Sửa bản xuất</button> : null}
        <VersionHistory result={result} shown={shown} items={kept} blocked={running || captioning || joining} onPlay={onPlayKept} onApply={onApplyVersion} onDelete={onDeleteKept} />
      </div>
      <div className="watch-media">
      {playable ? (
        isAudio(shown) ? (
          <ResultMedia key={`${shown}:${resultEpoch}`} kind="audio" src={src} mediaRef={mediaRef} onError={mediaFailed} />
        ) : (
          <div className="player-slot">
            <ResultMedia key={`${shown}:${resultEpoch}`} kind="video" src={src} mediaRef={mediaRef}
              onReady={mediaReady} onError={mediaFailed}
              onDownload={videoToSave(result, playingKept) ? onDownloadVideo : undefined} savingVideo={savingVideo} />
          </div>
        )
      ) : (
        <p className="library-empty">
          {isAudio(shown) ? "Dùng nút bên dưới nếu chưa nghe được." : "Dùng nút bên dưới nếu khung video còn trống."}
        </p>
      )}
      <div className="result-bar">
        <div className="result-actions">
          {playingKept ? (
            <>
              <div className="result-primary">
                <button type="button" className="check" onClick={() => void onOpen(playingKept)}>
                  Mở bằng trình phát
                </button>
                {saveVideo}
                {isAudio(playingKept) ? <button type="button" className="check" disabled={savingAudio} onClick={onDownloadAudio}>{savingAudio ? "Đang lưu…" : "Tải audio"}</button> : null}
              </div>
              <div className="result-tools">
                <button type="button" className="link" onClick={() => void onOpen(parentFolder(playingKept))}>
                  Mở folder
                </button>
                <button
                  type="button"
                  className="link"
                  onClick={() => {
                    const item = kept.find((row) => sameFile(row.path, playingKept));
                    if (item) void onDeleteKept(item);
                  }}
                >
                  Xóa bản này
                </button>
              </div>
            </>
          ) : (
            <>
              <div className="result-primary">
                {isAudio(result) && watched?.url ? (
                  <button type="button" className="check" onClick={() => void onFetchVideo()} disabled={running || downloading}>
                    Tải video
                  </button>
                ) : null}
                {isAudio(result) ? (
                  <button type="button" className="check" onClick={() => void onDownloadAudio()} disabled={savingAudio || running}>
                    {savingAudio ? "Đang xuất audio…" : "Tải audio"}
                  </button>
                ) : textOnly ? null : (
                  <button
                    type="button"
                    className="check"
                    onClick={() => void (listenFile ? onShowVideo() : onJoin())}
                    disabled={joining || running}
                  >
                    {joining
                      ? joinProgress === null
                        ? "Đang đọc liền mạch…"
                        : `Đang đọc liền mạch… ${Math.round(joinProgress * 100)}%`
                      : listenFile
                        ? "Xem video"
                        : "Nghe liền mạch"}
                  </button>
                )}
                {showListenStop(joining) ? (
                  <button type="button" className="link" onClick={() => void onStop()}>
                    Dừng
                  </button>
                ) : null}
                {isAudio(result) ? null : (
                  <button type="button" className="check" onClick={() => void onOpen(listenFile || result)} disabled={joining}>
                    Mở bằng trình phát
                  </button>
                )}
                {isAudio(result) || listenFile || captioning || textReady ? null : (
                  <button type="button" className="check" onClick={() => void onAddCaptions()} disabled={running}>
                    Thêm chữ vào video
                  </button>
                )}
                {saveVideo}
                {captioning ? (
                  <>
                    <span className="caption-wait">
                      {captionProgress === null ? "Đang ghi chữ…" : `Đang ghi chữ… ${Math.round(captionProgress * 100)}%`}
                    </span>
                    <button type="button" className="link" onClick={() => void onStop()}>
                      Dừng
                    </button>
                  </>
                ) : null}
                <button type="button" className="link result-again" onClick={() => void onAgain()} disabled={running || joining}>
                  Làm lại
                </button>
              </div>
              <div className="result-tools">
                {isAudio(result) ? (
                  <button type="button" className="link" onClick={() => void onOpen(result)}>
                    Mở bằng trình phát
                  </button>
                ) : textOnly ? null : (
                  <button
                    type="button"
                    className="link"
                    onClick={() => void onDownloadAudio()}
                    disabled={savingAudio || running || joining}
                  >
                    {savingAudio ? "Đang xuất audio…" : "Tải audio"}
                  </button>
                )}
                <button type="button" className="link" onClick={() => void onOpen(parentFolder(result))}>
                  Mở folder
                </button>
                <button
                  type="button"
                  className="link"
                  onClick={() =>
                    void onOpen(
                      (isListen(shown) ? shown : isAudio(result) ? result : plainVideo(result)).replace(/\.(mp4|m4a)$/i, ".srt"),
                    )
                  }
                >
                  Mở phụ đề .srt
                </button>
                {isAudio(result) || !textReady || textOnly ? null : (
                  <button
                    type="button"
                    className="link"
                    onClick={() => onShowResult(isCaptioned(result) ? plainVideo(result) : captionedVideo(result))}
                  >
                    {isCaptioned(result) ? "Xem bản không chữ" : "Xem bản có chữ"}
                  </button>
                )}
              </div>
            </>
          )}
        </div>
        {mediaNote ? <p className="error">{mediaNote}</p> : null}
        {openNote ? <p className="error">{openNote}</p> : null}
        {captionNote ? <p className="error">{captionNote}</p> : null}
        {error ? <p className="error">{error}</p> : null}
      </div>
      </div>
      {!playingKept && lines.length > 0 ? (
        <DubScript
          resultPath={result}
          lines={lines}
          videoRef={mediaRef}
          playable={playable}
          picks={picks}
          onPicks={onPicks}
          audioOnly={isAudio(result)}
          captionsOnly={textOnly}
          busy={running || captioning || joining}
          onRetake={onRetake}
          onStructure={onStructure}
        />
      ) : null}
    </div>
  );
}
