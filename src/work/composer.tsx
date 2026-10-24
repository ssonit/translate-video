import { DeepseekServerRow, useDeepseekServer } from "../DeepseekServer";
import LocalModels from "../LocalModels";
import { LockedNote } from "../SetupChip";
import { LOCAL_MODELS } from "../guides";
import { shareLink } from "../shareLink";
import { fileName, languageName, LANGUAGES } from "./format";
import { MODEL_HINTS, OTHER_MODEL } from "./models";
import { RunOptions } from "./options";
import type { QueueJob } from "../queue";

export function Composer({
  mode,
  onMode,
  composeVideo,
  composeLink,
  onLink,
  onPickVideo,
  audioOnly,
  onAudioOnly,
  textOnly,
  onTextOnly,
  skipTranslate,
  onSkip,
  burnCaptions,
  onBurn,
  captioning,
  running,
  folder,
  pending,
  provider,
  onProvider,
  keyDraft,
  onKeyDraft,
  onSaveKey,
  model,
  onModel,
  customModel,
  onCustom,
  onModelNote,
  onSaveModel,
  installed,
  modelListNote,
  copied,
  onCopy,
  onReloadModels,
  modelNote,
  voiceNote,
  prefsNote,
  error,
  queueNote,
  target,
  onTarget,
  voices,
  voice,
  previewing,
  onVoice,
  onPreview,
  queue,
  onDrop,
  onRun,
  onLive,
  lock,
  onTools,
  onChanged,
}: {
  mode: "file" | "link";
  onMode: (mode: "file" | "link") => void;
  composeVideo: string;
  composeLink: string;
  onLink: (value: string) => void;
  onPickVideo: () => void;
  audioOnly: boolean;
  onAudioOnly: (on: boolean) => void;
  textOnly: boolean;
  onTextOnly: (on: boolean) => void;
  skipTranslate: boolean;
  onSkip: (on: boolean) => void;
  burnCaptions: boolean;
  onBurn: (on: boolean) => void;
  captioning: boolean;
  running: boolean;
  folder: string;
  pending: string;
  provider: string;
  onProvider: (id: string) => void;
  keyDraft: string;
  onKeyDraft: (value: string) => void;
  onSaveKey: () => void;
  model: string;
  onModel: (value: string) => void;
  customModel: boolean;
  onCustom: (on: boolean) => void;
  onModelNote: (value: string) => void;
  onSaveModel: (value?: string) => void;
  installed: string[] | null;
  modelListNote: string;
  copied: string;
  onCopy: (command: string) => void;
  onReloadModels: () => void;
  modelNote: string;
  voiceNote: string;
  prefsNote: string;
  error: string;
  queueNote: string;
  target: string;
  onTarget: (code: string) => void;
  voices: string[];
  voice: string;
  previewing: boolean;
  onVoice: (name: string) => void;
  onPreview: () => void;
  queue: QueueJob[];
  onDrop: (id: string) => void;
  onRun: () => void;
  onLive: () => void;
  // Dòng báo thiếu mục bắt buộc. Trống thì không khóa.
  lock: string;
  onTools: () => void;
  // Bật hay tắt server DeepSeek làm kết quả kiểm tra máy đổi theo.
  onChanged: () => void;
}) {
  const hint = MODEL_HINTS[provider] ?? MODEL_HINTS.local;
  const local = provider === "local";
  const deepseek = useDeepseekServer(
    provider === "deepseek-local" && !skipTranslate,
  );
  const onMachine = (name: string) => installed?.includes(name) ?? false;
  const listedModels =
    local && installed !== null
      ? hint.models.filter((name) => onMachine(name))
      : hint.models;
  const chosenMissing =
    local &&
    installed !== null &&
    model !== "" &&
    hint.models.includes(model) &&
    !onMachine(model);
  const chosenLocal = local
    ? LOCAL_MODELS.find((item) => item.name === (model || hint.fallback))
    : undefined;
  const showCustomModel =
    customModel || (model !== "" && !hint.models.includes(model));
  const hasSource =
    mode === "file" ? composeVideo !== "" : composeLink.trim() !== "";
  const canRun = hasSource && folder !== "" && !captioning && !running;
  // Thiếu mục bắt buộc thì khóa các nút chạy. Chọn video, đổi dịch vụ, bật server vẫn làm được.
  const locked = lock !== "";

  return (
    <div className="pane-fill compose-pane">
      <div className="pane-scroll composer">
        <div className="source-heading">
          <p className="section-label">Nguồn video</p>
          <div className="picks" role="group" aria-label="Nguồn video">
            <button
              type="button"
              aria-pressed={mode === "link"}
              className={mode === "link" ? "pick is-on" : "pick"}
              onClick={() => onMode("link")}
            >
              Link video
            </button>
            <button
              type="button"
              aria-pressed={mode === "file"}
              className={mode === "file" ? "pick is-on" : "pick"}
              onClick={() => onMode("file")}
            >
              File trên máy
            </button>
          </div>
        </div>
        {mode === "file" ? (
          <article className="path source-card">
            <div>
              <strong>Video</strong>
              <p title={composeVideo}>
                {composeVideo
                  ? fileName(composeVideo)
                  : "Chọn một video để bắt đầu lồng tiếng."}
              </p>
            </div>
            <button
              type="button"
              className="link"
              onClick={() => void onPickVideo()}
            >
              {running ? "Thêm video" : "Chọn video"}
            </button>
          </article>
        ) : (
          <article className="path path-link source-card">
            <label className="source-link-label" htmlFor="compose-link">
              Link YouTube, Douyin hoặc TikTok
            </label>
            <input
              id="compose-link"
              className="link-input"
              type="text"
              value={composeLink}
              placeholder="Dán link hoặc cả đoạn chữ chia sẻ"
              aria-label="Link YouTube, Douyin hoặc TikTok"
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => onLink(event.target.value)}
              onPaste={(event) => {
                // Dán cả đoạn chia sẻ Douyin hoặc TikTok thì ô chỉ giữ link trong đó.
                const link = shareLink(event.clipboardData.getData("text"));
                if (link === null) return;
                event.preventDefault();
                onLink(link);
              }}
            />
            <p>
              {audioOnly
                ? "Chỉ lấy phần tiếng để nghe và xuất audio."
                : "Dán link một video, hoặc cả đoạn chia sẻ Douyin / TikTok."}
            </p>
          </article>
        )}
        <div className="compose-section">
          <p className="section-label">Ngôn ngữ & giọng đọc</p>
          <div className="tune path path-link">
            <label>
              {skipTranslate ? "Ngôn ngữ giọng" : "Dịch sang"}
              <select
                className="pill-select"
                value={target}
                aria-label={
                  skipTranslate ? "Ngôn ngữ giọng" : "Ngôn ngữ dịch sang"
                }
                onChange={(event) => onTarget(event.target.value)}
              >
                {LANGUAGES.map((item) => (
                  <option key={item.code} value={item.code}>
                    {item.name}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Giọng đọc
              {voices.length > 0 ? (
                <span className="voice-pick">
                  <select
                    className="pill-select"
                    value={voice}
                    aria-label="Chọn giọng đọc"
                    disabled={previewing || textOnly}
                    onChange={(event) => void onVoice(event.target.value)}
                  >
                    {voices.map((name) => (
                      <option key={name} value={name}>
                        {name}
                      </option>
                    ))}
                  </select>
                  <button
                    type="button"
                    onClick={() => void onPreview()}
                    disabled={running || previewing || voice === "" || textOnly}
                  >
                    {previewing ? "Đang đọc" : "Nghe thử"}
                  </button>
                </span>
              ) : (
                <p className="voice-empty">
                  Chưa có giọng {languageName(target)}. Thêm cặp file giọng vào
                  voices/{target}.
                </p>
              )}
            </label>
          </div>
          {textOnly ? (
            <p className="key-note">
              Chỉ hiện chữ: giữ tiếng gốc, không đọc giọng mới.
            </p>
          ) : null}
        </div>
        <div className="compose-section translation-section">
          <div className="run-options">
            <label
              className="redo"
              title="Video đã đúng ngôn ngữ muốn nghe. Từ Hán Việt và chữ Hán trong lời chép được giữ nguyên."
            >
              <input
                type="checkbox"
                checked={skipTranslate}
                onChange={(event) => void onSkip(event.target.checked)}
              />
              {textOnly
                ? "Không dịch, giữ lời gốc"
                : "Không dịch, chỉ đổi giọng"}
            </label>
          </div>
          {skipTranslate ? (
            <p className="key-note">
              {textOnly
                ? "Dùng lời gốc làm phụ đề. Không cần dịch vụ dịch."
                : "Chép lời và đọc lại bằng giọng đã chọn. Không cần dịch vụ dịch."}
            </p>
          ) : (
            <details
              className="translation-config"
              key={pending ? "needs-key" : "configured"}
              open={pending ? true : undefined}
            >
              <summary>
                <span>
                  <strong>
                    Dịch bằng{" "}
                    {MODEL_HINTS[pending || provider]?.name ?? hint.name}
                  </strong>
                  <small>Đổi dịch vụ, API key hoặc model</small>
                </span>
                <span className="config-chevron" aria-hidden="true">
                  ⌄
                </span>
              </summary>
              <div className="tune path path-link config-fields">
                <label>
                  Dịch bằng
                  <select
                    className="pill-select"
                    value={pending || provider}
                    aria-label="Dịch bằng"
                    onChange={(event) => void onProvider(event.target.value)}
                  >
                    {Object.entries(MODEL_HINTS).map(([id, item]) => (
                      <option key={id} value={id}>
                        {id === "local" ? "Trên máy" : item.name}
                      </option>
                    ))}
                  </select>
                </label>
                {pending ? (
                  <form
                    className="key-row"
                    onSubmit={(event) => {
                      event.preventDefault();
                      void onSaveKey();
                    }}
                  >
                    <input
                      type="password"
                      value={keyDraft}
                      placeholder={`API key ${MODEL_HINTS[pending].keyName ?? MODEL_HINTS[pending].name}`}
                      aria-label="API key"
                      autoComplete="off"
                      onChange={(event) => onKeyDraft(event.target.value)}
                    />
                    <button type="submit" className="link">
                      Lưu key
                    </button>
                  </form>
                ) : (
                  <label>
                    Model
                    <select
                      className="pill-select"
                      value={showCustomModel ? OTHER_MODEL : model}
                      aria-label="Model dịch"
                      onChange={(event) => {
                        const value = event.target.value;
                        if (value === OTHER_MODEL) {
                          onCustom(true);
                          onModelNote("");
                          return;
                        }
                        onCustom(false);
                        onModel(value);
                        void onSaveModel(value);
                      }}
                    >
                      {local &&
                      installed !== null &&
                      !onMachine(hint.fallback) &&
                      model === "" ? (
                        <option value="">Chọn model đã có</option>
                      ) : null}
                      {local &&
                      installed !== null &&
                      !onMachine(hint.fallback) ? null : (
                        <option value="">{`Mặc định (${hint.fallback})${local && installed !== null ? " · đã có" : ""}`}</option>
                      )}
                      {chosenMissing ? (
                        <option value={model}>{model} · chưa tải</option>
                      ) : null}
                      {listedModels
                        .filter(
                          (name) => name !== hint.fallback || model !== "",
                        )
                        .map((name) => (
                          <option key={name} value={name}>
                            {`${name}${local && installed !== null ? " · đã có" : ""}`}
                          </option>
                        ))}
                      <option value={OTHER_MODEL}>Khác…</option>
                    </select>
                  </label>
                )}
                {showCustomModel && !pending ? (
                  <label className="tune-wide">
                    Tên model
                    <input
                      className="link-input"
                      type="text"
                      value={model}
                      placeholder={
                        local
                          ? "Nhập tên model đã có trên máy"
                          : "Nhập tên model của dịch vụ"
                      }
                      aria-label="Tên model tự nhập"
                      autoComplete="off"
                      spellCheck={false}
                      onChange={(event) => {
                        onModel(event.target.value);
                        onModelNote("");
                      }}
                      onBlur={() => void onSaveModel()}
                    />
                  </label>
                ) : null}
                {local && !pending && chosenLocal && !showCustomModel ? (
                  <p className="key-note tune-wide">{chosenLocal.when}</p>
                ) : null}
                {local && !pending ? (
                  <div className="tune-wide">
                    <LocalModels
                      installed={installed}
                      note={modelListNote}
                      copied={copied}
                      onCopy={(command) => void onCopy(command)}
                      mode="missing"
                    />
                    <button
                      type="button"
                      className="link model-refresh"
                      onClick={onReloadModels}
                    >
                      Xem model đã tải
                    </button>
                  </div>
                ) : null}
                {provider === "deepseek-local" && !pending ? (
                  <div className="tune-wide deepseek-server">
                    <DeepseekServerRow
                      server={deepseek}
                      lockStop={running}
                      after={onChanged}
                    />
                    <p className="key-note">
                      Gọi server DeepSeek local trên máy, không cần API key.
                      deepseek-chat nhanh, deepseek-expert chậm hơn.
                    </p>
                  </div>
                ) : null}
              </div>
            </details>
          )}
        </div>
        <div className="compose-section output-section">
          <p className="section-label">Kết quả</p>
          <RunOptions
            audioOnly={audioOnly}
            textOnly={textOnly}
            burnCaptions={burnCaptions}
            disabled={captioning}
            onAudioOnly={onAudioOnly}
            onTextOnly={onTextOnly}
            onBurn={onBurn}
          />
        </div>
        <p className="preview-hint">
          {textOnly
            ? "Giữ tiếng gốc và ghi lời dịch lên video."
            : audioOnly
              ? "Nghe từng đoạn trong lúc dịch, xuất audio khi đã xong."
              : "Xem trước trong lúc dịch, ghép video khi đã sẵn sàng."}
        </p>
        {running ? (
          <p className="key-note">
            Video đang chạy giữ nguyên. Video thêm sau chờ đến lượt.
          </p>
        ) : null}
        {queue.length > 0 ? (
          <div className="wait-list">
            <p className="queue-label">Hàng đợi</p>
            {queue.map((item) => (
              <div key={item.id} className="wait-row">
                <span title={item.source}>
                  <strong>{item.label}</strong>
                  <em>
                    Đang chờ · {languageName(item.target)}
                    {item.audioOnly ? " · chỉ audio" : ""}
                    {item.textOnly ? " · chỉ chữ" : ""}
                    {item.skipTranslate ? " · chỉ đổi giọng" : ""}
                  </em>
                </span>
                <button
                  type="button"
                  aria-label={`Bỏ ${item.label}`}
                  onClick={() => onDrop(item.id)}
                >
                  Bỏ
                </button>
              </div>
            ))}
          </div>
        ) : null}
        {queueNote ? <p className="key-note">{queueNote}</p> : null}
        {modelNote ? <p className="key-note">{modelNote}</p> : null}
        {voiceNote ? <p className="error">{voiceNote}</p> : null}
        {prefsNote ? <p className="error">{prefsNote}</p> : null}
        {error ? <p className="error">{error}</p> : null}
      </div>
      <footer className="pane-foot compose-foot">
        <LockedNote note={lock} onTools={onTools} />
        <span className="foot-actions">
          <button
            type="button"
            className="link preview-action"
            onClick={() => void onLive()}
            disabled={locked || !canRun}
          >
            {audioOnly ? "Nghe trước" : "Xem trước"}
          </button>
          <button
            type="button"
            className={locked ? "check is-idle" : "check"}
            onClick={() =>
              void (running && mode === "file" && !hasSource
                ? onPickVideo()
                : onRun())
            }
            disabled={
              locked ||
              captioning ||
              folder === "" ||
              (running
                ? mode === "link" && !hasSource
                : !hasSource && queue.length === 0)
            }
          >
            {running
              ? mode === "file" && !hasSource
                ? "Thêm video"
                : "Thêm vào hàng"
              : hasSource || queue.length === 0
                ? "Chạy"
                : `Chạy hàng (${queue.length})`}
          </button>
        </span>
      </footer>
    </div>
  );
}
