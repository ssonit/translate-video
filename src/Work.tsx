import ListenPart from "./ListenPart";
import WatchLive from "./WatchLive";
import { AgainPane } from "./work/againPane";
import { Composer } from "./work/composer";
import { useWorkEffects } from "./work/effects";
import { fileName, isCaptioned, overallEstimate, plainVideo, sameFile } from "./work/format";
import { jobActions } from "./work/job";
import { Library } from "./work/library";
import { NameForm } from "./work/names";
import { playbackActions } from "./work/playback";
import { ProgressPane } from "./work/progressPane";
import { ResumePane } from "./work/resumePane";
import { SettingsPane } from "./work/settingsPane";
import { useWorkState } from "./work/state";
import type { FinishedDub } from "./work/types";
import { WatchPane } from "./work/watchPane";

export default function Work({
  installed,
  modelListNote,
  prefsEpoch,
  onSetup,
  onBusy,
  onReloadModels,
}: {
  installed: string[] | null;
  modelListNote: string;
  prefsEpoch: number;
  onSetup: () => void;
  onBusy: (text: string) => void;
  onReloadModels: () => void;
}) {
  const s = useWorkState();
  const job = jobActions(s);
  const play = playbackActions(s);
  const shown = s.playingKept || s.listenFile || s.result;
  const busyText = s.running
    ? s.downloading
      ? "Đang tải video"
      : `Đang lồng ${s.steps.filter((step) => step.state === "done" || step.state === "skipped").length}/${s.steps.length}`
    : "";
  useWorkEffects(s, {
    busyText,
    onBusy,
    prefsEpoch,
    shown,
    applyStatus: play.applyStatus,
    applyPrefs: play.applyPrefs,
    loadChoice: play.loadChoice,
    rememberQueue: job.rememberQueue,
  });

  const overall = overallEstimate(s.steps, s.runStartedAt, s.now);
  const activeStep = s.steps.find((step) => step.state === "running");
  const liveFraction = s.downloading ? s.download.progress : s.running && overall.fraction > 0 ? overall.fraction : null;
  const liveTitle = s.downloading ? "Đang tải" : s.video ? fileName(s.video) : "Video đang xử lý";
  const liveDetail = s.downloading
    ? s.download.detail || "Đang tải"
    : s.error && !s.running
      ? s.error
      : activeStep
        ? activeStep.label
        : s.running
          ? "Đang chạy"
          : "Lỗi";
  const showLive = s.running || s.downloading || s.error !== "";
  // Việc đang chạy đã có một dòng trong danh sách thì không vẽ thêm thẻ tiến độ.
  const isLiveItem = (item: FinishedDub) =>
    !item.done &&
    showLive &&
    ((item.video !== "" && sameFile(item.video, s.video)) || fileName(s.video).replace(/\.[^.]+$/, "") === item.title);
  const showLiveCard = showLive && !s.dubs.some((item) => isLiveItem(item));
  const previewSource = s.video || s.watchUpdate?.video || "";
  const previewRow = (item: FinishedDub) =>
    s.returnPreview &&
    !item.done &&
    previewSource !== "" &&
    ((item.video !== "" && sameFile(item.video, previewSource)) || fileName(previewSource).replace(/\.[^.]+$/, "") === item.title);
  const previewLabel = s.returnListen ? "Nghe trước" : "Xem trước";
  const watched = s.dubs.find(
    (item) => sameFile(item.path, s.result) || sameFile(item.path, plainVideo(s.result)),
  );
  const textReady = isCaptioned(s.result) || watched?.captioned === true;
  const update = s.watchUpdate;

  return (
    <main className="stage">
      <header className="mast stage-mast">
        <div className="mast-row">
          <div>
            <p className="kicker">Lồng tiếng</p>
            <h1>Video</h1>
          </div>
          <div className="mast-actions">
            <button type="button" className={s.view === "settings" ? "link is-on" : "link"} onClick={() => s.setView("settings")}>
              Cài đặt
            </button>
            <button type="button" className="link" onClick={onSetup}>
              Đồ nghề
            </button>
          </div>
        </div>
      </header>

      <div className="stage-body">
        <Library
          view={s.view}
          queueCount={s.queue.length}
          query={s.listQuery}
          onQuery={s.setListQuery}
          onNew={() => s.setView("new")}
          showLiveCard={showLiveCard}
          liveTitle={liveTitle}
          liveDetail={liveDetail}
          liveFraction={liveFraction}
          returnPreview={s.returnPreview}
          previewLabel={previewLabel}
          onProgress={() => s.setView("progress")}
          onLive={() => s.setView("live")}
          dubs={s.dubs}
          note={s.listNote}
          removing={s.removing}
          result={s.result}
          draft={s.draft}
          captioning={s.captioning}
          isLive={isLiveItem}
          isPreview={previewRow}
          onOpenDone={(item) => {
            s.setResult(item.path);
            s.setResultTextOnly(isCaptioned(item.path));
            s.setOpenNote("");
            s.setError("");
            s.setView("watch");
          }}
          onOpenDraft={(item) => {
            s.setDraft(item);
            s.setView("resume");
          }}
          onRemove={(item) => void play.removeDub(item)}
        />

        <section className="pane">
          {s.nameEdit ? (
            <div className="pane-fill">
              <NameForm
                title="Sửa tên"
                intro="Tên đúng dùng cho phụ đề và giọng. Lưu chỉ ghi danh sách, không xóa bản dịch."
                note=""
                error={s.nameNote}
                rows={s.nameRows}
                disabled={s.nameSending}
                onChange={s.setNameRows}
                primary="Lưu"
                secondary="Huỷ"
                onPrimary={() => void play.saveEditedNames()}
                onSecondary={play.closeNameEdit}
              />
            </div>
          ) : null}

          {!s.nameEdit && s.againOpen && s.view === "watch" && s.result ? (
            <AgainPane
              summary={play.againSummary()}
              lang={s.againLang}
              voice={s.againVoice}
              voices={s.againVoices}
              retranscribe={s.retranscribe}
              textOnly={s.againTextOnly}
              note={s.againNote}
              rows={s.againNames}
              running={s.running}
              onLang={s.setAgainLang}
              onVoice={s.setAgainVoice}
              onRetranscribe={s.setRetranscribe}
              onTextOnly={s.setAgainTextOnly}
              onRows={s.setAgainNames}
              onRun={() => void play.runAgain()}
              onClose={() => {
                s.setAgainOpen(false);
                s.setAgainNote("");
              }}
            />
          ) : null}

          {!s.nameEdit && !s.againOpen && s.view === "watch" && s.result ? (
            <WatchPane
              shown={shown}
              result={s.result}
              playingKept={s.playingKept}
              listenFile={s.listenFile}
              lines={s.lines}
              joining={s.joining}
              joinProgress={s.joinProgress}
              playable={s.playable}
              resultEpoch={s.resultEpoch}
              mediaRef={s.mediaRef}
              kept={s.kept}
              watched={watched}
              savingAudio={s.savingAudio}
              running={s.running}
              downloading={s.downloading}
              captioning={s.captioning}
              captionProgress={s.captionProgress}
              textReady={textReady}
              textOnlyRun={s.resultTextOnly}
              openNote={s.openNote}
              captionNote={s.captionNote}
              error={s.error}
              picks={s.picks}
              onPicks={s.setPicks}
              onPlayCurrent={() => s.setPlayingKept("")}
              onPlayKept={s.setPlayingKept}
              onDeleteKept={(item) => void play.deleteKept(item)}
              onOpen={(path) => void play.openResult(path)}
              onFetchVideo={() => void job.fetchVideo()}
              onDownloadAudio={() => void play.downloadAudio()}
              onJoin={() => void play.joinListen()}
              onShowVideo={play.showVideo}
              onStop={() => void job.stop()}
              onAddCaptions={() => void play.addCaptions()}
              onAgain={() => void play.openAgain()}
              onShowResult={(path) => {
                s.setResult(path);
                s.setResultTextOnly(false);
              }}
              onRetake={(edits) => void play.retakeLines(edits)}
            />
          ) : null}

          {!s.nameEdit && s.view === "progress" ? (
            <ProgressPane
              downloading={s.downloading}
              download={s.download}
              running={s.running}
              error={s.error}
              liveFraction={liveFraction}
              liveDetail={liveDetail}
              elapsed={overall.elapsed}
              remaining={overall.remaining}
              fraction={overall.fraction}
              started={s.runStartedAt !== null}
              queueCount={s.queue.length}
              steps={s.steps}
              now={s.now}
              canRetry={s.retryRef.current !== null}
              nameReview={s.nameReview}
              nameRows={s.nameRows}
              nameNote={s.nameNote}
              nameSending={s.nameSending}
              onRetry={() => void job.retryLast()}
              onNames={s.setNameRows}
              onSubmit={(keep) => void play.submitNames(keep)}
              onStop={() => void job.stop()}
              onRunQueue={() => void job.runQueue()}
              onNew={() => s.setView("new")}
            />
          ) : null}

          {!s.nameEdit && s.view === "parts" && s.partsRoot ? (
            <ListenPart
              root={s.partsRoot}
              lang={s.target}
              voice={s.voice}
              skipTranslate={s.skipTranslate}
              onBack={() => s.setView("new")}
            />
          ) : null}

          {!s.nameEdit && s.view === "live" ? (
            <div className="pane-fill">
              {s.error ? <p className="error">{s.error}</p> : null}
              {update ? (
                <WatchLive
                  update={update}
                  listen={s.audioOnly}
                  onStop={() => void job.stop()}
                  onExport={() => void job.runFrom(s.video || update.video)}
                />
              ) : (
                <p className="live-wait">Đang chép đoạn đầu</p>
              )}
            </div>
          ) : null}

          {!s.nameEdit && s.view === "settings" ? (
            <SettingsPane
              folder={s.folder}
              running={s.running}
              useTerms={s.useTerms}
              skipTranslate={s.skipTranslate}
              note={s.prefsNote}
              onFolder={() => void job.pickFolder()}
              onTerms={(on) => void job.chooseTerms(on)}
            />
          ) : null}

          {!s.nameEdit && s.view === "resume" && s.draft ? (
            <ResumePane
              draft={s.draft}
              audioOnly={s.audioOnly}
              textOnly={s.textOnly}
              burnCaptions={s.burnCaptions}
              captioning={s.captioning}
              running={s.running}
              onAudioOnly={job.chooseAudioOnly}
              onTextOnly={job.chooseTextOnly}
              onBurn={(checked) => void job.chooseBurn(checked)}
              onNames={(root) => void play.openNames(root)}
              onLeave={() => s.setView("new")}
              onResume={(item) => void job.resume(item)}
            />
          ) : null}

          {!s.nameEdit && s.view === "new" ? (
            <Composer
              mode={s.mode}
              onMode={s.setMode}
              composeVideo={s.composeVideo}
              composeLink={s.composeLink}
              onLink={s.setComposeLink}
              onPickVideo={() => void job.pickVideo()}
              audioOnly={s.audioOnly}
              onAudioOnly={job.chooseAudioOnly}
              textOnly={s.textOnly}
              onTextOnly={job.chooseTextOnly}
              skipTranslate={s.skipTranslate}
              onSkip={(on) => void job.chooseSkip(on)}
              burnCaptions={s.burnCaptions}
              onBurn={(checked) => void job.chooseBurn(checked)}
              captioning={s.captioning}
              running={s.running}
              folder={s.folder}
              pending={s.pending}
              provider={s.provider}
              onProvider={(id) => void play.chooseProvider(id)}
              keyDraft={s.keyDraft}
              onKeyDraft={s.setKeyDraft}
              onSaveKey={() => void play.saveKey()}
              model={s.model}
              onModel={s.setModel}
              customModel={s.customModel}
              onCustom={s.setCustomModel}
              onModelNote={s.setModelNote}
              onSaveModel={(value) => void play.saveModel(value)}
              installed={installed}
              modelListNote={modelListNote}
              copied={s.copied}
              onCopy={(command) => void play.copyModelCommand(command)}
              onReloadModels={onReloadModels}
              modelNote={s.modelNote}
              voiceNote={s.voiceNote}
              prefsNote={s.prefsNote}
              error={s.error}
              queueNote={s.queueNote}
              target={s.target}
              onTarget={play.chooseTarget}
              voices={s.voices}
              voice={s.voice}
              previewing={s.previewing}
              onVoice={(name) => void play.chooseVoice(name)}
              onPreview={() => void play.previewVoice()}
              queue={s.queue}
              onDrop={job.dropQueued}
              onRun={() => void job.run()}
              onLive={() => void job.runLive()}
            />
          ) : null}
        </section>
      </div>
    </main>
  );
}
