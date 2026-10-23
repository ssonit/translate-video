import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { GUIDES, OPTIONAL_TOOLS, guideFor, type CloudChoice, type ToolId } from "./guides";
import LocalModels from "./LocalModels";
import Work from "./Work";
import { useTtsBridge } from "./tts/bridge";
import "./App.css";

type ToolStatus = {
  id: ToolId;
  ready: boolean;
  detail: string;
};

type Provider = "local" | CloudChoice["id"];

type InstallEvent = {
  stage: "check" | "program" | "model" | "done" | "error";
  detail: string;
  done: number;
  total: number;
};

export default function App() {
  useTtsBridge();
  const [tools, setTools] = useState<ToolStatus[]>([]);
  const [selected, setSelected] = useState<ToolId>("ffmpeg");
  const [checking, setChecking] = useState(true);
  const [error, setError] = useState("");
  const [copied, setCopied] = useState("");
  const [provider, setProvider] = useState<Provider>("local");
  const [apiKey, setApiKey] = useState("");
  const [keyNote, setKeyNote] = useState("");
  const [screen, setScreen] = useState<"setup" | "work">("setup");
  const [openedWork, setOpenedWork] = useState(false);
  // Work stays mounted once opened, so a run keeps its state while the user looks at the tools.
  const [workMounted, setWorkMounted] = useState(false);
  const [busy, setBusy] = useState("");
  const [voices, setVoices] = useState<string[]>([]);
  const [voice, setVoice] = useState("");
  const [previewing, setPreviewing] = useState(false);
  const [voiceNote, setVoiceNote] = useState("");
  const [installing, setInstalling] = useState(false);
  const [install, setInstall] = useState<InstallEvent | null>(null);
  const [installError, setInstallError] = useState("");
  const [installed, setInstalled] = useState<string[] | null>(null);
  const [modelListNote, setModelListNote] = useState("");
  const [skipTranslate, setSkipTranslate] = useState(false);
  const [prefsEpoch, setPrefsEpoch] = useState(0);

  const loadModels = useCallback(async () => {
    try {
      setInstalled(await invoke<string[]>("list_local_models"));
      setModelListNote("");
    } catch (caught) {
      setInstalled(null);
      setModelListNote(typeof caught === "string" ? caught : "Chưa đọc được danh sách model.");
    }
  }, []);

  const check = useCallback(async (keepSelection = false) => {
    setChecking(true);
    setError("");
    try {
      const next = await invoke<ToolStatus[]>("check_setup");
      setTools(next);
      await loadModels();
      if (!keepSelection) {
        const firstGap = next.find((tool) => !tool.ready && !OPTIONAL_TOOLS.includes(tool.id));
        if (firstGap) setSelected(firstGap.id);
      }
    } catch {
      setError("Mở bằng cửa sổ app để kiểm tra máy. Trang Vite một mình không đọc được PATH.");
    } finally {
      setChecking(false);
    }
  }, [loadModels]);

  useEffect(() => {
    void check();
  }, [check]);

  useEffect(() => {
    invoke<string[]>("list_voices", { lang: "vi" })
      .then(setVoices)
      .catch(() => {});
    invoke<string>("get_voice", { lang: "vi" })
      .then(setVoice)
      .catch(() => {});
  }, []);

  useEffect(() => {
    const stop = listen<InstallEvent>("whisper-install", (event) => setInstall(event.payload));
    return () => {
      void stop.then((off) => off());
    };
  }, []);

  useEffect(() => {
    if (screen !== "setup") return;
    invoke<{ skipTranslate: boolean }>("get_prefs")
      .then((prefs) => setSkipTranslate(prefs.skipTranslate))
      .catch(() => {});
  }, [screen]);

  useEffect(() => {
    invoke<{ provider: Provider; apiKey: string }>("get_translation_choice")
      .then((choice) => {
        setProvider(choice.provider);
        setApiKey(choice.apiKey);
      })
      .catch(() => {});
  }, []);

  const guide = guideFor(selected);
  const status = tools.find((tool) => tool.id === selected);
  const required = tools.filter((tool) => !OPTIONAL_TOOLS.includes(tool.id));
  const readyCount = required.filter((tool) => tool.ready).length;
  const ready = required.length > 0 && readyCount === required.length;

  useEffect(() => {
    if (screen === "work") setWorkMounted(true);
  }, [screen]);

  useEffect(() => {
    if (!openedWork && !checking && ready) {
      setOpenedWork(true);
      setScreen("work");
    }
  }, [openedWork, checking, ready]);

  async function openLink(url: string) {
    try {
      await openUrl(url);
    } catch {
      window.open(url, "_blank", "noopener,noreferrer");
    }
  }

  async function chooseVoice(name: string) {
    setVoiceNote("");
    try {
      await invoke("save_voice", { lang: "vi", name });
      setVoice(name);
    } catch (caught) {
      setVoiceNote(typeof caught === "string" ? caught : "Chưa lưu được giọng.");
    }
  }

  async function previewVoice() {
    setVoiceNote("");
    setPreviewing(true);
    try {
      const wav = await invoke<ArrayBuffer>("preview_voice", { lang: "vi", name: voice });
      const url = URL.createObjectURL(new Blob([wav], { type: "audio/wav" }));
      const audio = new Audio(url);
      audio.onended = () => URL.revokeObjectURL(url);
      await audio.play();
    } catch (caught) {
      setVoiceNote(typeof caught === "string" ? caught : "Chưa đọc thử được giọng.");
    } finally {
      setPreviewing(false);
    }
  }

  async function installWhisper() {
    setInstallError("");
    setInstall(null);
    setInstalling(true);
    try {
      await invoke<string>("install_whisper");
      await check(true);
    } catch (caught) {
      setInstallError(typeof caught === "string" ? caught : "Chưa tải được Whisper.");
    } finally {
      setInstalling(false);
    }
  }

  async function copyCommand(command: string) {
    await navigator.clipboard.writeText(command);
    setCopied(command);
    window.setTimeout(() => setCopied(""), 1600);
  }

  async function chooseSkip(on: boolean) {
    setSkipTranslate(on);
    try {
      const prefs = await invoke<{ skipTranslate: boolean }>("save_prefs", { skipTranslate: on });
      setSkipTranslate(prefs.skipTranslate);
      setPrefsEpoch((epoch) => epoch + 1);
      await check(true);
    } catch (caught) {
      setSkipTranslate(!on);
      setError(typeof caught === "string" ? caught : "Chưa lưu được lựa chọn dịch.");
    }
  }

  async function saveProvider(next: Provider, key: string) {
    setKeyNote("");
    try {
      await invoke("save_translation_choice", { provider: next, apiKey: key });
      setProvider(next);
      setApiKey(next === "local" ? "" : key.trim());
      setKeyNote(next === "local" ? "" : "Đã lưu key trên máy.");
      await check(true);
    } catch (caught) {
      setKeyNote(typeof caught === "string" ? caught : "Chưa lưu được key.");
    }
  }

  const work =
    screen === "work" || workMounted ? (
      <div style={{ display: screen === "work" ? "contents" : "none" }}>
        <Work
          installed={installed}
          modelListNote={modelListNote}
          prefsEpoch={prefsEpoch}
          onSetup={() => setScreen("setup")}
          onBusy={setBusy}
          onReloadModels={() => void loadModels()}
        />
      </div>
    ) : null;

  // Same tree in both screens (a fragment with Work first), or React would remount Work on every switch.
  return (
    <>
    {work}
    {screen === "work" ? null : (
    <main className="desk">
      <header className="mast">
        <p className="kicker">Trước khi lồng tiếng</p>
        <div className="mast-row">
          <h1>Đủ đồ nghề chưa</h1>
          <div className="mast-actions">
            {busy ? (
              <button type="button" className="link" onClick={() => setScreen("work")}>
                {busy}, xem tiến độ
              </button>
            ) : ready ? (
              <button type="button" className="link" onClick={() => setScreen("work")}>
                Lồng tiếng
              </button>
            ) : null}
            <button type="button" className="check" onClick={() => void check()} disabled={checking}>
              {checking ? "Đang kiểm tra" : "Kiểm tra lại"}
            </button>
          </div>
        </div>
        <p className="lead">
          Máy tự đối chiếu các công cụ. Mục nào thiếu, bấm vào để xem cách tải. Bạn tự tải file về máy.
        </p>
        <p className="count">
          {tools.length === 0 ? "Chưa có kết quả" : `${readyCount}/${required.length} đã có`}
        </p>
        {error ? <p className="error">{error}</p> : null}
      </header>

      <section className="sheet">
        <ol className="list">
          {GUIDES.map((item, index) => {
            const itemStatus = tools.find((tool) => tool.id === item.id);
            const ready = itemStatus?.ready ?? false;
            return (
              <li key={item.id}>
                <button
                  type="button"
                  className={selected === item.id ? "row is-on" : "row"}
                  onClick={() => setSelected(item.id)}
                >
                  <span className="index">{String(index + 1).padStart(2, "0")}</span>
                  <span>
                    <strong>{item.name}</strong>
                    <em>{item.role}</em>
                  </span>
                  <span className={ready ? "mark is-ready" : "mark"}>
                    {ready ? "Có" : OPTIONAL_TOOLS.includes(item.id) ? "Tùy chọn" : "Thiếu"}
                  </span>
                </button>
              </li>
            );
          })}
        </ol>

        <article className="guide">
          <p className="kicker">
            {status?.ready ? (guide.clouds && provider !== "local" ? "Dịch qua mạng" : "Đã có trên máy") : "Cách tải"}
          </p>
          <h2>{guide.name}</h2>
          <p className="detail">{status?.detail ?? guide.role}</p>
          {guide.id === "model" ? (
            <label className="redo settings-terms">
              <input
                type="checkbox"
                checked={skipTranslate}
                onChange={(event) => void chooseSkip(event.target.checked)}
              />
              Không dịch, chỉ đổi giọng
            </label>
          ) : null}
          {guide.id === "model" ? (
            <p className="settings-note">
              Bật khi video đã là ngôn ngữ muốn nghe, kể cả lời nhiều từ Hán Việt. Không cần model dịch.
            </p>
          ) : null}
          {status?.ready ? (
            <p className="done">Mục này xong. Chọn dòng còn thiếu nếu vẫn còn.</p>
          ) : (
            <ol>
              {guide.steps.map((step) => (
                <li key={step}>{step}</li>
              ))}
            </ol>
          )}
          {guide.clouds ? (
            <div className="picks">
              <button
                type="button"
                className={provider === "local" ? "pick is-on" : "pick"}
                onClick={() => void saveProvider("local", "")}
              >
                Trên máy
              </button>
              {guide.clouds.map((cloud) => (
                <button
                  key={cloud.id}
                  type="button"
                  className={provider === cloud.id ? "pick is-on" : "pick"}
                  onClick={() => {
                    if (provider === cloud.id) return;
                    setProvider(cloud.id);
                    setApiKey("");
                    setKeyNote("");
                  }}
                >
                  {cloud.name}
                </button>
              ))}
            </div>
          ) : null}
          {guide.id === "model" && provider === "local" ? (
            <LocalModels
              installed={installed}
              note={modelListNote}
              copied={copied}
              onCopy={(command) => void copyCommand(command)}
              mode="all"
            />
          ) : null}
          {guide.command && provider === "local" ? (
            <div className="command">
              <div>
                <strong>{guide.commandName}</strong>
                {guide.commandNote ? <p>{guide.commandNote}</p> : null}
                <code>{guide.command}</code>
              </div>
              <button type="button" onClick={() => void copyCommand(guide.command!)}>
                {copied === guide.command ? "Đã chép" : "Chép lệnh"}
              </button>
            </div>
          ) : null}
          {guide.id === "voice" && voices.length > 0 ? (
            <div className="command voice-row">
              <div>
                <strong>Giọng đọc</strong>
                <p>Giọng này dùng khi lồng tiếng. Bấm Thử giọng để nghe một câu mẫu.</p>
                <select
                  value={voice}
                  aria-label="Chọn giọng đọc"
                  disabled={previewing}
                  onChange={(event) => void chooseVoice(event.target.value)}
                >
                  {voices.map((name) => (
                    <option key={name} value={name}>
                      {name}
                    </option>
                  ))}
                </select>
              </div>
              <button type="button" onClick={() => void previewVoice()} disabled={previewing || voice === ""}>
                {previewing ? "Đang đọc" : "Thử giọng"}
              </button>
            </div>
          ) : null}
          {guide.id === "whisper" ? (
            <div className="command install-row">
              <div>
                <strong>{status?.ready ? "Kiểm tra bản mới" : "Tải Whisper tự động"}</strong>
                <p>
                  App kiểm tra bản mới nhất, chỉ tải phần còn thiếu hoặc cũ hơn (tối đa khoảng 475 MB) từ github.com và
                  huggingface.co, đối chiếu mã băm rồi tự giải nén vào folder whisper. Không gửi dữ liệu nào của bạn đi.
                </p>
                {installing || install ? (
                  <>
                    <progress
                      max={install && install.total > 0 ? install.total : undefined}
                      value={install && install.total > 0 ? install.done : undefined}
                    />
                    <p className="install-note">
                      {install?.detail ?? "Đang bắt đầu..."}
                      {install && install.total > 0
                        ? ` ${Math.round((install.done / install.total) * 100)}%`
                        : ""}
                    </p>
                  </>
                ) : null}
              </div>
              {installing ? (
                <button type="button" onClick={() => void invoke("cancel_whisper_install")}>
                  Dừng
                </button>
              ) : (
                <button type="button" onClick={() => void installWhisper()}>
                  {status?.ready ? "Kiểm tra" : "Tải Whisper"}
                </button>
              )}
            </div>
          ) : null}
          {guide.id === "whisper" && installError ? <p className="error install-error">{installError}</p> : null}
          {guide.id === "voice" && voiceNote ? <p className="key-note">{voiceNote}</p> : null}
          {guide.clouds && provider !== "local" ? (
            <form
              className="key-row"
              onSubmit={(event) => {
                event.preventDefault();
                void saveProvider(provider, apiKey);
              }}
            >
              <input
                type="password"
                value={apiKey}
                placeholder="API key"
                autoComplete="off"
                onChange={(event) => setApiKey(event.target.value)}
              />
              <button type="submit">Lưu key</button>
              <button
                type="button"
                className="link"
                onClick={() => {
                  const cloud = guide.clouds?.find((item) => item.id === provider);
                  if (cloud) void openLink(cloud.keyUrl);
                }}
              >
                Lấy API key
              </button>
            </form>
          ) : null}
          {keyNote && guide.clouds ? <p className="key-note">{keyNote}</p> : null}
          {guide.clouds && provider !== "local" ? (
            <p className="key-note">Chỉ gửi chữ lời thoại. Video ở lại trên máy.</p>
          ) : null}
          {guide.links.length > 0 ? (
            <div className="links">
              {guide.links.map((link) => (
                <button key={link.url} type="button" className="link" onClick={() => void openLink(link.url)}>
                  {link.label}
                </button>
              ))}
            </div>
          ) : null}
        </article>
      </section>
    </main>
    )}
    </>
  );
}
