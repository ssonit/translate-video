import { DeepseekServerRow, useDeepseekServer } from "../DeepseekServer";
import LocalModels from "../LocalModels";
import { LockedNote } from "../SetupChip";
import { LOCAL_MODELS } from "../guides";
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
  const deepseek = useDeepseekServer(provider === "deepseek-local" && !skipTranslate);
  const onMachine = (name: string) => installed?.includes(name) ?? false;
  const listedModels = local && installed !== null ? hint.models.filter((name) => onMachine(name)) : hint.models;
  const chosenMissing = local && installed !== null && model !== "" && hint.models.includes(model) && !onMachine(model);
  const chosenLocal = local ? LOCAL_MODELS.find((item) => item.name === (model || hint.fallback)) : undefined;
  const showCustomModel = customModel || (model !== "" && !hint.models.includes(model));
  const hasSource = mode === "file" ? composeVideo !== "" : composeLink.trim() !== "";
  const canRun = hasSource && folder !== "" && !captioning && !running;
  // Thiếu mục bắt buộc thì khóa các nút chạy. Chọn video, đổi dịch vụ, bật server vẫn làm được.
  const locked = lock !== "";

  return (
    <div className="pane-fill">
      <div className="pane-scroll composer hug">
        <div className="picks">
          <button type="button" className={mode === "link" ? "pick is-on" : "pick"} onClick={() => onMode("link")}>
            Link video
          </button>
          <button type="button" className={mode === "file" ? "pick is-on" : "pick"} onClick={() => onMode("file")}>
            File trên máy
          </button>
        </div>
        {mode === "file" ? (
          <article className="path">
            <div>
              <strong>Video</strong>
              <p title={composeVideo}>{composeVideo ? fileName(composeVideo) : "Chưa chọn"}</p>
            </div>
            <button type="button" className="check" onClick={() => void onPickVideo()}>
              {running ? "Thêm video" : "Chọn video"}
            </button>
          </article>
        ) : (
          <article className="path path-link">
            <strong>Link YouTube, Douyin hoặc TikTok</strong>
            <input
              className="link-input"
              type="text"
              value={composeLink}
              placeholder="Dán link hoặc cả đoạn chữ chia sẻ"
              aria-label="Link YouTube, Douyin hoặc TikTok"
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => onLink(event.target.value)}
            />
            <p>
              {audioOnly
                ? "Không tải video. Link YouTube được lưu. Có phụ đề thì dùng phụ đề, không có thì tải tiếng."
                : "Tải vào thư mục tai-ve. YouTube và TikTok dùng yt-dlp. Douyin dùng Edge."}
            </p>
          </article>
        )}
        <div className="tune path path-link">
          <div className="run-options tune-wide">
            <label className="redo" title="Video đã đúng ngôn ngữ muốn nghe. Từ Hán Việt và chữ Hán trong lời chép được giữ nguyên.">
              <input type="checkbox" checked={skipTranslate} onChange={(event) => void onSkip(event.target.checked)} />
              Không dịch, chỉ đổi giọng
            </label>
          </div>
          {skipTranslate ? (
            <p className="key-note tune-wide">Whisper chép lời, giọng đã chọn đọc lại. Không chạy bước dịch và không cần model.</p>
          ) : (
            <>
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
                    {local && installed !== null && !onMachine(hint.fallback) && model === "" ? (
                      <option value="">Chọn model đã có</option>
                    ) : null}
                    {local && installed !== null && !onMachine(hint.fallback) ? null : (
                      <option value="">{`Mặc định (${hint.fallback})${local && installed !== null ? " · đã có" : ""}`}</option>
                    )}
                    {chosenMissing ? <option value={model}>{model} · chưa tải</option> : null}
                    {listedModels
                      .filter((name) => name !== hint.fallback || model !== "")
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
                    placeholder="Gõ tên model local khác, ví dụ llama3.1:8b"
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
              {local && !pending && chosenLocal && !showCustomModel ? <p className="key-note tune-wide">{chosenLocal.when}</p> : null}
              {local && !pending ? (
                <div className="tune-wide">
                  <LocalModels installed={installed} note={modelListNote} copied={copied} onCopy={(command) => void onCopy(command)} mode="missing" />
                  <button type="button" className="link model-refresh" onClick={onReloadModels}>
                    Xem model đã tải
                  </button>
                </div>
              ) : null}
              {provider === "deepseek-local" && !pending ? (
                <div className="tune-wide deepseek-server">
                  <DeepseekServerRow server={deepseek} lockStop={running} after={onChanged} />
                  <p className="key-note">
                    Gọi server DeepSeek local trên máy, không cần API key. deepseek-chat nhanh, deepseek-expert chậm hơn.
                  </p>
                </div>
              ) : null}
            </>
          )}
          <label>
            {skipTranslate ? "Ngôn ngữ giọng" : "Dịch sang"}
            <select
              className="pill-select"
              value={target}
              aria-label={skipTranslate ? "Ngôn ngữ giọng" : "Ngôn ngữ dịch sang"}
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
                  disabled={previewing}
                  onChange={(event) => void onVoice(event.target.value)}
                >
                  {voices.map((name) => (
                    <option key={name} value={name}>
                      {name}
                    </option>
                  ))}
                </select>
                <button type="button" onClick={() => void onPreview()} disabled={running || previewing || voice === ""}>
                  {previewing ? "Đang đọc" : "Thử"}
                </button>
              </span>
            ) : (
              <p>Chưa có giọng cho {languageName(target)}. Bỏ cặp file vào voices\{target}.</p>
            )}
          </label>
        </div>
        <RunOptions
          audioOnly={audioOnly}
          textOnly={textOnly}
          burnCaptions={burnCaptions}
          disabled={captioning}
          onAudioOnly={onAudioOnly}
          onTextOnly={onTextOnly}
          onBurn={onBurn}
        />
        {running ? <p className="key-note">Video đang chạy giữ nguyên. Video thêm sau chờ đến lượt.</p> : null}
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
                <button type="button" aria-label={`Bỏ ${item.label}`} onClick={() => onDrop(item.id)}>
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
      <footer className="pane-foot">
        <LockedNote note={lock} onTools={onTools} />
        <span className="foot-actions">
          <button
            type="button"
            className={locked ? "check is-idle" : "check"}
            onClick={() => void (running && mode === "file" && !hasSource ? onPickVideo() : onRun())}
            disabled={locked || captioning || folder === "" || (running ? mode === "link" && !hasSource : !hasSource && queue.length === 0)}
          >
            {running ? (mode === "file" && !hasSource ? "Thêm video" : "Thêm vào hàng") : hasSource ? "Chạy" : `Chạy hàng (${queue.length})`}
          </button>
          <button type="button" className={locked ? "check is-idle" : "check"} onClick={() => void onLive()} disabled={locked || !canRun}>
            {audioOnly ? "Nghe trước" : "Xem trước"}
          </button>
        </span>
      </footer>
    </div>
  );
}
