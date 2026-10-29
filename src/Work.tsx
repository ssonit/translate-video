import { ask } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import { StudioNav } from "./work/StudioNav";
import { ConfigHelp } from "./work/ConfigHelp";
import { UserGuide } from "./work/UserGuide";
import { PresetLibrary } from "./work/PresetLibrary";
import type { RenderSettings } from "./work/renderSettings";
import { QueuePanel } from "./work/QueuePanel";
import { ProductionOptions } from "./work/ProductionOptions";
import { StyleEditor } from "./work/StyleEditor";
import ListenPart from "./ListenPart";
import { SetupChip } from "./SetupChip";
import { lockNote, type SetupSummary } from "./setupStatus";
import WatchLive from "./WatchLive";
import { AgainPane } from "./work/againPane";
import { Composer } from "./work/composer";
import { useWorkEffects } from "./work/effects";
import { watchDataKey, watchMediaKey } from "./work/watchLoad";
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
  setup,
  onSetup,
  onChanged,
  onBusy,
  onReloadModels,
}: {
  installed: string[] | null;
  modelListNote: string;
  prefsEpoch: number;
  setup: SetupSummary;
  onSetup: () => void;
  // Đổi dịch vụ, key hay Không dịch làm kết quả kiểm tra máy đổi theo, nên xin App kiểm tra lại.
  onChanged: () => void;
  onBusy: (text: string) => void;
  onReloadModels: () => void;
}) {
  const s = useWorkState();
  const listenOptions = useRef({ target: s.target, voice: s.voice, skipTranslate: s.skipTranslate });
  if (s.view === "parts") listenOptions.current = { target: s.target, voice: s.voice, skipTranslate: s.skipTranslate };
  const [configTab, setConfigTab] = useState<"visual" | "audio" | "sample">("visual");
  const [configOrigin, setConfigOrigin] = useState<"new" | "watch">("new");
  useEffect(() => {
    if (s.view !== "watch") s.mediaRef.current?.pause();
  }, [s.view, s.mediaRef]);
  const job = jobActions(s);
  const play = playbackActions(s);
  const textDraft = useRef(false);
  const reportTextDraft = (active: boolean) => { textDraft.current = active; };
  async function confirmDiscard() {
    if (!textDraft.current && Object.keys(s.picks).length === 0) return true;
    return ask("Video đang mở có nháp lời hoặc mốc chưa xuất. Tiếp tục sẽ bỏ những thay đổi này. Bạn có thể quay về Video đang mở để xuất trước.", {
      title: "Thay đổi chưa xuất", kind: "warning", okLabel: "Bỏ nháp và tiếp tục", cancelLabel: "Giữ nháp",
    });
  }
  async function openVideo(path: string) {
    if (!sameFile(path, s.result) && !await confirmDiscard()) return;
    s.setResult(path); s.setResultTextOnly(isCaptioned(path));
    s.setOpenNote(""); s.setError(""); s.setView("watch");
  }
  const latest = useRef({ result: s.result, busy: false });
  latest.current = { result: s.result, busy: s.running || s.captioning || s.joining };
  async function editExport(versionPath?: string){
    if (latest.current.busy) return;
    const source = s.result;
    s.setOpenNote("");
    try{
      const cfg=await invoke<{video:string;outputDir:string;target:string;voice:string;useTerms:boolean;skipTranslate:boolean;textOnly:boolean;audioOnly:boolean;burnCaptions:boolean;render:RenderSettings}>(versionPath ? "version_config" : "read_production",versionPath ? {result:source,path:versionPath} : {path:source});
      if (latest.current.busy || latest.current.result !== source) return;
      await invoke("save_prefs",{useTerms:cfg.useTerms,skipTranslate:cfg.skipTranslate,burnCaptions:cfg.burnCaptions});
      if(s.target!==cfg.target)s.pendingVoice.current={lang:cfg.target,name:cfg.voice};
      s.setTarget(cfg.target);s.setVoice(cfg.voice);s.setFolder(cfg.outputDir);s.setMode("file");s.setComposeVideo(cfg.video);
      s.setUseTerms(cfg.useTerms);s.setSkipTranslate(cfg.skipTranslate);s.setTextOnly(cfg.textOnly);s.setAudioOnly(cfg.audioOnly);s.setBurnCaptions(cfg.burnCaptions);
      job.chooseRender(cfg.render);setConfigOrigin("watch");s.setView("configure");onChanged();
    }catch(e){s.setOpenNote(String(e));}
  }
  const lock = lockNote(setup);
  const shown = s.playingKept || s.listenFile || s.result;
  const busyText = s.running
    ? s.downloading
      ? "Đang tải video"
      : s.nameReview ? "Đang chờ bạn kiểm tra tên"
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

  const overall = overallEstimate(s.steps, s.runStartedAt, s.now, s.runPause);
  const activeStep = s.steps.find((step) => step.state === "running");
  const liveFraction = s.downloading ? s.download.progress : s.running && overall.fraction > 0 ? overall.fraction : null;
  const liveTitle = s.downloading ? "Đang tải" : s.video ? fileName(s.video) : "Video đang xử lý";
  const liveDetail = s.downloading
    ? s.download.detail || "Đang tải"
    : s.error && !s.running
      ? s.error
      : s.nameReview ? "Đang chờ bạn kiểm tra tên"
      : activeStep
        ? activeStep.label
        : s.running
          ? "Đang chạy"
          : s.result ? "Đã hoàn tất" : "Sẵn sàng";
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
    <main className={`stage studio-shell studio-view-${s.view}`}>
      <StudioNav view={s.view} count={s.queue.length} running={s.running || s.downloading} preview={s.returnPreview ? "live" : s.partsRoot ? "parts" : null} previewLabel={s.partsRoot && !s.returnPreview ? "Nghe trước" : previewLabel} detail={liveDetail} result={!!s.result}
        onView={view => { s.setNameEdit(false); s.setAgainOpen(false); s.setView(view); }} />
      <div className="studio-content">
      <header className="mast stage-mast">
        <div className="mast-row">
          <div className="studio-breadcrumb"><span>Không gian làm việc</span><span aria-hidden="true">/</span>
            <strong>{{ new: "Lồng tiếng", progress: "Hàng đợi", library: "Thư viện", presets: "Bộ cài đặt", configure: "Cấu hình bản xuất", settings: "Cài đặt", help: "Hướng dẫn sử dụng", watch: "Video đang mở", resume: "Chạy tiếp", live: "Xem trước", parts: "Nghe trước" }[s.view]}</strong>
          </div>
          <div className="mast-actions"><SetupChip summary={setup} onClick={onSetup} /></div>
        </div>
      </header>
      <div className="stage-body">
        <div className="studio-library-page" hidden={s.view !== "library"}>
          <header className="studio-page-heading"><p className="kicker">Video của bạn</p><h2>Thư viện</h2><p>Kết quả đã xuất và các việc đang dở, lưu trên máy.</p></header>
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
          onOpenDone={item => void openVideo(item.path)}
          onOpenDraft={(item) => {
            s.setDraft(item);
            s.setView("resume");
          }}
          onRemove={(item) => void play.removeDub(item)}
        />
        </div>

        <section className="pane" hidden={s.view === "library"}>
          {s.view === "help" ? <UserGuide onTools={onSetup} onStart={() => s.setView("new")} onSettings={() => s.setView("settings")} /> : null}
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

          {s.result ? (
            <div className="studio-result-host" hidden={s.nameEdit || s.againOpen || s.view !== "watch"}>
            <WatchPane
              key={watchMediaKey(shown, s.resultEpoch)}
              shown={shown}
              result={s.result}
              playingKept={s.playingKept}
              listenFile={s.listenFile}
              lines={s.lines}
              joining={s.joining}
              joinProgress={s.joinProgress}
              playable={s.playable && s.checkedMedia === watchMediaKey(shown, s.resultEpoch)}
              loading={s.checkedMedia !== watchMediaKey(shown, s.resultEpoch)
                || s.loadedWatchData !== watchDataKey(s.result, s.listenFile, s.resultEpoch)}
              resultEpoch={s.resultEpoch}
              mediaRef={s.mediaRef}
              kept={s.kept}
              watched={watched}
              savingAudio={s.savingAudio}
              savingVideo={s.savingVideo}
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
              onPlayCurrent={() => void play.selectVersion("")}
              onPlayKept={(path) => void play.selectVersion(path)}
              onDeleteKept={(item) => void play.deleteKept(item)}
              onOpen={(path) => void play.openResult(path)}
              onFetchVideo={() => void job.fetchVideo()}
              onDownloadAudio={() => void play.downloadAudio()}
              onDownloadVideo={() => void play.downloadVideo()}
              onJoin={() => void play.joinListen()}
              onShowVideo={play.showVideo}
              onStop={() => void job.stop()}
              onAddCaptions={() => void play.addCaptions()}
              onAgain={() => void play.openAgain()}
              onEditExport={() => void editExport()}
              onApplyVersion={(path) => void editExport(path)}
              onShowResult={(path) => {
                s.setResult(path);
                s.setResultTextOnly(false);
              }}
              onTextDraft={reportTextDraft}
              onRetake={(edits) => void play.retakeLines(edits)}
              onStructure={(action) => void play.retakeLines([], action)}
            />
            </div>
          ) : null}

          {!s.nameEdit && s.view === "progress" ? (
            <div className="studio-queue-page pane-scroll">
            <header className="studio-page-heading"><p className="kicker">Theo dõi xử lý</p><h2>Hàng đợi</h2><p>Tiến độ và cấu hình của từng video ở cùng một nơi.</p></header>
            <ProgressPane lock={lock} onTools={onSetup}
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
              onRetry={() => void confirmDiscard().then(ok => { if (ok) void job.retryLast(); })}
              onNames={s.setNameRows}
              onSubmit={(keep) => void play.submitNames(keep)}
              onStop={() => void job.stop()}
              onSkipCurrent={() => void job.skipCurrent()}
              onRunQueue={() => void confirmDiscard().then(ok => { if (ok) void job.runQueue(); })}
              onNew={() => s.setView("new")}
            />
            <QueuePanel queue={s.queue} history={s.queueHistory} policy={s.queuePolicy} presets={s.presets} running={s.running} needsKey={!s.skipTranslate && s.pending!==""}
                onPolicy={job.chooseQueuePolicy} onMove={job.moveQueued} onDrop={job.dropQueued} onUpdate={job.updateQueued} onBulk={job.bulkPreset} onRetry={r=>void confirmDiscard().then(ok => { if (ok) void job.retryRecord(r); })} onOpen={path=>void openVideo(path)}/>
            {!s.running && s.queue.length === 0 ? <div className="studio-empty"><strong>Không có video đang chờ</strong><p>Thêm nguồn ở màn Lồng tiếng để bắt đầu một việc mới.</p></div> : null}
            {s.queueNote ? <p role="status" className="key-note">{s.queueNote}</p> : null}
            </div>
          ) : null}

          {s.partsRoot ? (
            <div className="studio-preview-host" hidden={s.nameEdit || s.view !== "parts"}>
            <ListenPart active={s.view === "parts" && !s.nameEdit}
              root={s.partsRoot}
              lang={listenOptions.current.target}
              voice={listenOptions.current.voice}
              skipTranslate={listenOptions.current.skipTranslate}
              onBack={() => s.setView("new")}
            />
            </div>
          ) : null}

          <div className="studio-preview-host" hidden={s.nameEdit || s.view !== "live"}>
            <div className="pane-fill">
              {s.error ? <p className="error">{s.error}</p> : null}
              {update ? (
                <WatchLive active={s.view === "live" && !s.nameEdit}
                  update={update}
                  listen={s.audioOnly}
                  onStop={() => void job.stop()}
                  onExport={() => void job.runFrom(s.video || update.video)}
                />
              ) : (
                <p className="live-wait">Đang chép đoạn đầu</p>
              )}
            </div>
          </div>

          {!s.nameEdit && s.view === "settings" ? (
            <SettingsPane
              folder={s.folder}
              running={s.running}
              useTerms={s.useTerms}
              skipTranslate={s.skipTranslate}
              note={s.prefsNote}
              onFolder={() => void job.pickFolder()}
              onTerms={(on) => void job.chooseTerms(on)}
              onBack={() => s.setView("new")}
              onTools={onSetup}
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
              onResume={(item) => void confirmDiscard().then(ok => { if (ok) void job.resume(item); })}
              lock={lock}
              onTools={onSetup}
            />
          ) : null}

          <div className="studio-compose-host" hidden={s.nameEdit || s.view !== "new"}>
            <Composer
              queueTools={<p className="key-note">{s.queue.length} video đang chờ · <button type="button" className="link" onClick={() => s.setView("progress")}>Mở hàng đợi</button></p>}
              styleEditor={<><div className="studio-preset-quick"><label>Bộ cài đặt<select aria-label="Áp dụng nhanh bộ cài đặt" value="" disabled={s.running || s.captioning || s.presetApplying} onChange={event => { const preset = s.presets.find(p => p.id === event.target.value); if (preset) void job.applyPreset(preset).then(onChanged); }}><option value="">Lựa chọn hiện tại</option>{s.presets.map(p => <option key={p.id} value={p.id}>{p.name}</option>)}</select></label><button type="button" className="link" onClick={() => s.setView("presets")}>Quản lý bộ</button></div>
                <div className="studio-config-entry">
                  <button type="button" className="studio-config-link" onClick={() => { setConfigOrigin("new"); s.setView("configure"); }}>
                    <span><strong>Hình ảnh, chữ & âm thanh</strong><small>Đổi khung video, chỉnh phụ đề và âm lượng</small></span><span aria-hidden="true">↗</span>
                  </button>
                  <ConfigHelp />
                </div>
              </>}
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
              onSkip={(on) => void job.chooseSkip(on).then(onChanged)}
              burnCaptions={s.burnCaptions}
              onBurn={(checked) => void job.chooseBurn(checked)}
              captioning={s.captioning || s.presetApplying}
              running={s.running}
              folder={s.folder}
              pending={s.pending}
              provider={s.provider}
              onProvider={(id) => void play.chooseProvider(id).then(onChanged)}
              keyDraft={s.keyDraft}
              onKeyDraft={s.setKeyDraft}
              onSaveKey={() => void play.saveKey().then(onChanged)}
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
              onRun={() => void confirmDiscard().then(ok => { if (ok) void job.run(); })}
              onLive={() => void job.runLive()}
              lock={lock}
              onTools={onSetup}
              onChanged={onChanged}
            />
          </div>
          <div className="studio-config-page" hidden={s.view !== "configure" || s.nameEdit}>
            <div className="pane-scroll studio-page">
              <header className="studio-page-heading"><p className="kicker">Cấu hình bản xuất</p><h2>Hình ảnh & âm thanh</h2><p>Áp dụng cho {configOrigin === "watch" ? "lần xuất lại video đã chọn" : "video sắp chạy"}. Bản đang phát chỉ thay đổi sau khi xuất thành công.</p><p className="studio-config-source" title={s.mode === "file" ? s.composeVideo : s.composeLink}>{s.mode === "file" ? fileName(s.composeVideo) || "Chưa chọn nguồn" : s.composeLink || "Chưa chọn nguồn"}</p></header>
              <div className="studio-tabs" aria-label="Nhóm cấu hình">{([ ["visual", "Hình & chữ"], ["audio", "Âm thanh"], ["sample", "Đoạn thử 8 giây"] ] as const).map(([id, label]) => <button type="button" aria-pressed={configTab === id} className={configTab === id ? "is-active" : ""} onClick={() => setConfigTab(id)} key={id}>{label}</button>)}</div>
              <p className="studio-config-intro">{{
                visual: "Chỉnh khung video, phụ đề, logo và vùng che.",
                audio: "Chỉnh tiếng gốc, giọng lồng và cách phát âm tên.",
                sample: "Xuất thử 8 giây để kiểm tra trước khi làm cả video.",
              }[configTab]}</p>
              <div hidden={configTab !== "visual"}><StyleEditor standalone  audioOnly={s.audioOnly} value={s.render} onChange={job.chooseRender}
                source={s.mode === "file" ? s.composeVideo : ""} disabled={s.captioning || s.running || s.presetApplying}
                captions={!s.audioOnly && (s.textOnly || s.burnCaptions)} presets={s.presets} onSave={job.savePreset}
                onApply={(p) => void job.applyPreset(p).then(onChanged)} onDelete={job.deletePreset} /></div>
              <ProductionOptions section={configTab} value={s.render} onChange={job.chooseRender} lang={s.target} voice={s.voice}
                source={s.mode === "file" ? s.composeVideo : ""} folder={s.folder} audioOnly={s.audioOnly} textOnly={s.textOnly} burn={s.burnCaptions}
                skipTranslate={s.skipTranslate} useTerms={s.useTerms} disabled={s.running || s.captioning || s.presetApplying} onBusy={s.setCaptioning} />
            </div>
            <footer className="studio-export-bar"><p>{s.prefsNote || "Cấu hình được giữ trên máy. Các video đã xếp hàng không thay đổi."}</p><button type="button" className="link" onClick={() => s.setView(configOrigin)}>Quay lại</button>
              {configOrigin === "watch" ? <button type="button" className="check" disabled={!!lock || s.running || s.captioning || s.presetApplying} onClick={() => void job.run()}>Xuất theo cấu hình</button> : <button type="button" className="check" onClick={() => s.setView("new")}>Dùng cấu hình này</button>}
              {lock ? <button type="button" className="link" onClick={onSetup}>Xem đồ nghề</button> : null}
            </footer>
          </div>
          {s.view === "presets" ? <PresetLibrary presets={s.presets} disabled={s.running || s.captioning || s.presetApplying} note={s.prefsNote}
            onSave={job.savePreset} onDelete={job.deletePreset} onRename={job.renamePreset}
            onConfigure={() => { setConfigOrigin("new"); s.setView("configure"); }}
            onApply={p => void job.applyPreset(p).then(() => { onChanged(); s.setView("new"); })} /> : null}
        </section>
      </div>
      </div>
    </main>
  );
}
