import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { TtsEngine, STOPPED } from "./tts/engine";
import type { VoiceFiles } from "./tts/engine";
import type { VoiceConfig, Pcm16 } from "./tts/piper";
import { wavBytes } from "./tts/wav";
import { acceptChunk } from "./tts/streamGate";

type Outline = {
  index: number;
  count: number;
  words: number;
  source: string;
  text: string;
};

async function fetchVoice(lang: string, name: string): Promise<VoiceFiles> {
  const [model, config] = await Promise.all([
    invoke<ArrayBuffer>("read_voice", { lang, name, kind: "model" }),
    invoke<ArrayBuffer>("read_voice", { lang, name, kind: "config" }),
  ]);
  return { model, config: JSON.parse(new TextDecoder().decode(config)) as VoiceConfig };
}

function errorText(caught: unknown) {
  if (caught instanceof Error) return caught.message;
  return typeof caught === "string" ? caught : "Không nghe được phần này.";
}

export default function ListenPart({
  root,
  lang,
  voice,
  skipTranslate,
  onBack,
  active = true,
}: {
  root: string;
  lang: string;
  voice: string;
  skipTranslate: boolean;
  onBack: () => void;
  active?: boolean;
}) {
  const engine = useRef<TtsEngine | null>(null);
  if (!engine.current) engine.current = new TtsEngine(fetchVoice);
  const urls = useRef<string[]>([]);
  const cursor = useRef(0);
  const node = useRef<HTMLAudioElement | null>(null);
  const paused = useRef(false);
  const gen = useRef(0);
  const [index, setIndex] = useState(1);
  const [count, setCount] = useState(0);
  const [words, setWords] = useState(0);
  const [source, setSource] = useState("");
  const [text, setText] = useState("");
  const [note, setNote] = useState("");
  const [error, setError] = useState("");
  const [translating, setTranslating] = useState(false);
  const [making, setMaking] = useState(false);
  const [playing, setPlaying] = useState(false);
  const [hasAudio, setHasAudio] = useState(false);
  const [spoken, setSpoken] = useState("");
  const [spokenVoice, setSpokenVoice] = useState("");

  useEffect(() => {
    if (!active) {
      paused.current = true;
      node.current?.pause();
      setPlaying(false);
    }
  }, [active]);

  function stopNode() {
    node.current?.pause();
    node.current = null;
  }

  function clearAudio() {
    paused.current = true;
    stopNode();
    setPlaying(false);
    for (const url of urls.current) URL.revokeObjectURL(url);
    urls.current = [];
    cursor.current = 0;
    setHasAudio(false);
    setSpoken("");
  }

  function playFrom(at: number) {
    const url = urls.current[at];
    if (!url) {
      setPlaying(false);
      cursor.current = 0;
      return;
    }
    cursor.current = at;
    const audio = new Audio(url);
    node.current = audio;
    audio.onended = () => {
      if (node.current !== audio) return;
      node.current = null;
      if (paused.current) {
        cursor.current = at + 1;
        return;
      }
      playFrom(at + 1);
    };
    setPlaying(true);
    void audio.play().catch(() => setPlaying(false));
  }

  function pushChunk(pcm: Pcm16) {
    const tail = Math.round(pcm.rate * 0.28);
    const samples = new Int16Array(pcm.samples.length + tail);
    samples.set(pcm.samples);
    const bytes = wavBytes(pcm.rate, samples);
    const copy = new ArrayBuffer(bytes.byteLength);
    new Uint8Array(copy).set(bytes);
    const url = URL.createObjectURL(new Blob([copy], { type: "audio/wav" }));
    urls.current.push(url);
    setHasAudio(true);
    if (!paused.current && !node.current) playFrom(urls.current.length - 1);
  }

  useEffect(() => {
    const current = engine.current;
    return () => {
      gen.current += 1;
      current?.stop();
      node.current?.pause();
      for (const url of urls.current) URL.revokeObjectURL(url);
    };
  }, []);

  useEffect(() => {
    const token = ++gen.current;
    engine.current?.cancel();
    clearAudio();
    paused.current = false;
    setError("");
    setNote("");
    setText("");
    setMaking(false);
    setTranslating(true);
    void (async () => {
      try {
        const outline = await invoke<Outline>("listen_part", { root, part: index, target: lang });
        if (gen.current !== token) return;
        setCount(outline.count);
        setWords(outline.words);
        setSource(outline.source);
        if (outline.text || skipTranslate) {
          setText(skipTranslate ? outline.source : outline.text);
          setTranslating(false);
          return;
        }
        setNote("Đang dịch phần này");
        const translated = await invoke<string>("translate_listen_part", { root, part: index, target: lang });
        if (gen.current !== token) return;
        setText(translated);
        setNote("");
      } catch (caught) {
        if (gen.current !== token) return;
        setError(errorText(caught));
        setNote("");
      } finally {
        if (gen.current === token) setTranslating(false);
      }
    })();
    // clearAudio is stable enough for this load; recreating it would restart the request.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [root, index, lang, skipTranslate]);

  useEffect(() => {
    if (!voice) return;
    void engine.current?.load(lang, voice).catch(() => {});
  }, [lang, voice]);

  async function create() {
    const body = text.trim();
    if (!body || !voice) return;
    const token = ++gen.current;
    engine.current?.cancel();
    clearAudio();
    paused.current = false;
    setPlaying(true);
    setMaking(true);
    setError("");
    setSpoken(body);
    setSpokenVoice(voice);
    try {
      await engine.current?.load(lang, voice);
      if (gen.current !== token) return;
      let heard = false;
      await engine.current?.speakStream(body, lang, (pcm) => {
        if (!acceptChunk(token, gen.current)) return;
        heard = true;
        pushChunk(pcm);
      });
      if (gen.current === token && !heard) {
        setError("Không có chữ để đọc.");
        setPlaying(false);
      }
    } catch (caught) {
      if (gen.current !== token) return;
      const message = errorText(caught);
      if (message !== STOPPED) {
        setError(message);
        setPlaying(false);
      }
    } finally {
      if (gen.current === token) setMaking(false);
    }
  }

  function pause() {
    paused.current = true;
    node.current?.pause();
    setPlaying(false);
  }

  function resume() {
    paused.current = false;
    if (node.current) {
      setPlaying(true);
      void node.current.play().catch(() => setPlaying(false));
      return;
    }
    playFrom(cursor.current);
  }

  const same = hasAudio && text.trim() === spoken && voice === spokenVoice;
  const label = playing ? "Tạm dừng" : same ? "Nghe" : count > 1 ? "Tạo audio phần này" : "Tạo audio";

  return (
    <div className="pane-fill part-listen">
      <div className="part-head">
        <div>
          <p className="kicker">Nghe trước</p>
          <h2>{count > 1 ? `Phần ${index}` : "Phần này"}</h2>
        </div>
        <button type="button" className="link" onClick={onBack}>
          Quay lại
        </button>
      </div>
      {count > 1 ? (
        <div className="part-nav">
          <button type="button" className="link" disabled={index <= 1 || translating} onClick={() => setIndex(index - 1)}>
            Phần trước
          </button>
          <span>
            {index} / {count}
            {words > 0 ? ` · khoảng ${words.toLocaleString("vi-VN")} từ` : ""}
          </span>
          <button type="button" className="link" disabled={index >= count || translating} onClick={() => setIndex(index + 1)}>
            Phần sau
          </button>
        </div>
      ) : null}
      <p className="part-note">{note || "Phát ngay từng câu để nghe. Chưa ghi file audio."}</p>
      <p className="preview-hint">Bản nghe trước chưa qua kiểm tra tên và thuật ngữ như khi xuất audio.</p>
      {error ? <p className="error">{error}</p> : null}
      <textarea
        className="part-text"
        value={translating && !text ? source : text}
        readOnly={translating}
        onChange={(event) => setText(event.target.value)}
        aria-label="Lời của phần này"
      />
      <div className="part-actions">
        <button
          type="button"
          className="check part-play"
          disabled={translating || (making && !hasAudio) || !text.trim() || voice === ""}
          onClick={() => {
            if (playing) pause();
            else if (same) resume();
            else void create();
          }}
        >
          <svg viewBox="0 0 20 20" aria-hidden="true">
            {playing ? <path d="M5 4h3.2v12H5zm6.8 0H15v12h-3.2z" /> : <path d="M6 4.2v11.6l9-5.8z" />}
          </svg>
          {making && !hasAudio ? "Đang tạo…" : label}
        </button>
        {voice === "" ? <span className="part-note">Chưa chọn giọng.</span> : null}
      </div>
    </div>
  );
}
