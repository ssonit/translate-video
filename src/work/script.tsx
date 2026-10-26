import { useEffect, useMemo, useRef, useState, type RefObject } from "react";
import { pickedEdits } from "./format";
import { fitWarning, nextFitWarning } from "./fitWarning";
import type { LinePick, ScriptLine } from "./types";

function clock(seconds: number) {
  const total = Math.max(0, Math.floor(seconds));
  const minutes = Math.floor(total / 60);
  const secs = total % 60;
  return `${minutes}:${String(secs).padStart(2, "0")}`;
}

function lineAt(lines: ScriptLine[], time: number) {
  for (let index = 0; index < lines.length; index++) {
    const line = lines[index];
    if (time >= line.start && time < line.end) return index;
  }
  return -1;
}

function rowInView(list: HTMLElement, row: HTMLElement) {
  const listBox = list.getBoundingClientRect();
  const rowBox = row.getBoundingClientRect();
  const visible = Math.min(rowBox.bottom, listBox.bottom) - Math.max(rowBox.top, listBox.top);
  return visible >= Math.min(rowBox.height, listBox.height) * 0.6;
}

function keepLineVisible(list: HTMLElement, row: HTMLElement) {
  const gap = 6;
  const listBox = list.getBoundingClientRect();
  const rowBox = row.getBoundingClientRect();
  if (rowBox.top < listBox.top + gap) list.scrollTop -= listBox.top + gap - rowBox.top;
  else if (rowBox.bottom > listBox.bottom - gap) list.scrollTop += rowBox.bottom - (listBox.bottom - gap);
}

// Menu tốc độ (nút ba chấm) là popup của trình phát, mở lên và tràn ra ngoài khung
// video. Đổi câu phụ đề hoặc cuộn danh sách thì popup đóng. Giữ danh sách yên suốt
// lúc menu còn mở. Chuột rời khung không được thả, vì người dùng đang rê vào menu.
const CONTROLS_HOLD_MS = 45000;
const CONTROLS_RELEASE_MS = 300;

function inOverflowButton(node: HTMLMediaElement, event: PointerEvent) {
  const box = node.getBoundingClientRect();
  const minY = node instanceof HTMLAudioElement ? box.top - 4 : box.bottom - 72;
  return (
    event.clientX >= box.right - 180 &&
    event.clientX <= box.right + 48 &&
    event.clientY >= minY &&
    event.clientY <= box.bottom + 20
  );
}

function inOpenMenu(node: HTMLMediaElement, event: PointerEvent) {
  const box = node.getBoundingClientRect();
  if (node instanceof HTMLAudioElement) {
    return (
      event.clientX >= box.left - 24 &&
      event.clientX <= box.right + 420 &&
      event.clientY >= box.top - 520 &&
      event.clientY <= box.bottom + 28
    );
  }
  return (
    event.clientX >= box.right - 460 &&
    event.clientX <= box.right + 420 &&
    event.clientY >= box.bottom - 560 &&
    event.clientY <= box.bottom + 36
  );
}

export function DubScript({
  lines,
  videoRef,
  playable,
  picks,
  onPicks,
  audioOnly,
  captionsOnly,
  busy,
  onRetake,
}: {
  lines: ScriptLine[];
  videoRef: RefObject<HTMLMediaElement | null>;
  playable: boolean;
  picks: Record<number, LinePick>;
  onPicks: (picks: Record<number, LinePick>) => void;
  audioOnly: boolean;
  captionsOnly: boolean;
  busy: boolean;
  onRetake: (edits: { index: number; text: string }[]) => void;
}) {
  const [spoken, setSpoken] = useState(-1);
  const scriptRef = useRef<HTMLOListElement>(null);
  const rowRefs = useRef<Array<HTMLLIElement | null>>([]);
  const followRef = useRef(true);
  const holdRef = useRef(-1);
  const scrollLock = useRef(0);
  const spokenRef = useRef(-1);
  const usingControls = useRef(false);
  const controlsTimer = useRef(0);
  const keptRate = useRef({ rate: 1, src: "" });
  const [editing, setEditing] = useState<number | null>(null);
  const [draft, setDraft] = useState("");
  const reviewRef = useRef(-1);
  const warnings = useMemo(
    () => lines.map((line) => captionsOnly || audioOnly ? "" : fitWarning(line)),
    [lines, captionsOnly, audioOnly],
  );
  const warningIndexes = warnings.flatMap((warning, index) => warning ? [index] : []);
  spokenRef.current = spoken;
  const edits = pickedEdits(lines, picks);

  function holdScroll() {
    scrollLock.current += 1;
    window.setTimeout(() => {
      scrollLock.current = Math.max(0, scrollLock.current - 1);
    }, 80);
  }

  useEffect(() => {
    reviewRef.current = -1;
    followRef.current = true;
    holdRef.current = -1;
    holdScroll();
    if (!usingControls.current) scriptRef.current?.scrollTo(0, 0);
  }, [lines]);

  useEffect(() => {
    const node = videoRef.current;
    const sync = () => {
      if (usingControls.current) return;
      const index = lineAt(lines, node?.currentTime ?? 0);
      setSpoken((prev) => (prev === index ? prev : index));
      if (index < 0 || followRef.current || index === holdRef.current) return;
      const list = scriptRef.current;
      const row = rowRefs.current[index];
      if (list && row && rowInView(list, row)) followRef.current = true;
    };
    // Cùng một file mà trình phát tự tải lại thì tốc độ bị về 1. Gắn lại tốc độ vừa chọn.
    // ratechange lúc đang tải là máy tự đặt về 1, không phải người dùng chọn.
    let loading = false;
    const onLoadStart = () => {
      loading = true;
    };
    const onLoaded = () => {
      if (!node) return;
      const src = node.currentSrc;
      const kept = keptRate.current;
      if (src && src === kept.src) {
        if (kept.rate > 0 && node.playbackRate !== kept.rate) node.playbackRate = kept.rate;
      } else if (src) {
        keptRate.current = { rate: node.playbackRate || 1, src };
      }
      loading = false;
      sync();
    };
    const scheduleRelease = (delay: number) => {
      window.clearTimeout(controlsTimer.current);
      controlsTimer.current = window.setTimeout(() => {
        usingControls.current = false;
        sync();
      }, delay);
    };
    const arm = () => {
      usingControls.current = true;
      window.clearTimeout(controlsTimer.current);
      controlsTimer.current = window.setTimeout(() => {
        usingControls.current = false;
        sync();
      }, CONTROLS_HOLD_MS);
    };
    const onRate = () => {
      if (!node || loading || !node.playbackRate) return;
      keptRate.current = { rate: node.playbackRate, src: node.currentSrc || keptRate.current.src };
      if (usingControls.current) scheduleRelease(CONTROLS_RELEASE_MS);
    };
    sync();
    if (!node) return;
    let lastDown = 0;
    const onPointerDown = (event: PointerEvent) => {
      const now = performance.now();
      if (now - lastDown < 50) return;
      lastDown = now;
      // Bấm lại đúng nút ba chấm thì menu đóng. Bấm trong menu (kể cả tràn ra ngoài khung) thì giữ.
      if (inOverflowButton(node, event)) {
        if (usingControls.current) scheduleRelease(CONTROLS_RELEASE_MS);
        else arm();
        return;
      }
      if (usingControls.current && inOpenMenu(node, event)) {
        arm();
        return;
      }
      if (usingControls.current) scheduleRelease(CONTROLS_RELEASE_MS);
    };
    const onPointerMove = (event: PointerEvent) => {
      if (!usingControls.current) return;
      if (inOpenMenu(node, event) || inOverflowButton(node, event)) arm();
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && usingControls.current) scheduleRelease(CONTROLS_RELEASE_MS);
    };
    node.addEventListener("timeupdate", sync);
    node.addEventListener("seeked", sync);
    const onError = () => {
      loading = false;
    };
    node.addEventListener("loadstart", onLoadStart);
    node.addEventListener("error", onError);
    node.addEventListener("loadedmetadata", onLoaded);
    node.addEventListener("ratechange", onRate);
    window.addEventListener("pointerdown", onPointerDown, true);
    document.addEventListener("pointermove", onPointerMove);
    document.addEventListener("keydown", onKey);
    return () => {
      window.clearTimeout(controlsTimer.current);
      usingControls.current = false;
      node.removeEventListener("timeupdate", sync);
      node.removeEventListener("seeked", sync);
      node.removeEventListener("loadstart", onLoadStart);
      node.removeEventListener("error", onError);
      node.removeEventListener("loadedmetadata", onLoaded);
      node.removeEventListener("ratechange", onRate);
      window.removeEventListener("pointerdown", onPointerDown, true);
      document.removeEventListener("pointermove", onPointerMove);
      document.removeEventListener("keydown", onKey);
    };
  }, [lines, playable, videoRef]);

  useEffect(() => {
    if (usingControls.current || !followRef.current || spoken < 0) return;
    const list = scriptRef.current;
    const row = rowRefs.current[spoken];
    if (!list || !row) return;
    holdScroll();
    keepLineVisible(list, row);
  }, [spoken]);

  function onScriptScroll() {
    if (scrollLock.current > 0) return;
    followRef.current = false;
    holdRef.current = spokenRef.current;
  }

  function playLine(index: number) {
    const node = videoRef.current;
    const line = lines[index];
    if (!node || !line) return;
    usingControls.current = false;
    window.clearTimeout(controlsTimer.current);
    followRef.current = true;
    holdRef.current = -1;
    holdScroll();
    node.currentTime = line.start;
    setSpoken(index);
    const list = scriptRef.current;
    const row = rowRefs.current[index];
    if (list && row) keepLineVisible(list, row);
  }

  function openEdit(index: number) {
    const line = lines[index];
    if (!line || busy) return;
    setEditing(index);
    setDraft(picks[index]?.text ?? line.translation);
  }

  function commitEdit(again: boolean) {
    if (editing === null) return;
    const line = lines[editing];
    if (!line) return;
    const text = draft.trim();
    const same = text === line.translation.trim();
    const next = { ...picks };
    if (!again && same) delete next[editing];
    else next[editing] = { text, again: again || !same };
    onPicks(next);
    setEditing(null);
  }

  return (
    <div className="script-wrap">
      {warningIndexes.length > 0 ? (
        <div className="script-review" role="note" aria-label="Câu cần nghe lại">
          <p><strong>{warningIndexes.length} câu cần nghe lại</strong><span>Có câu đã tăng tốc hoặc bị cắt. Bấm Sửa để rút gọn rồi đọc lại.</span></p>
          <button type="button" className="link" disabled={!playable || editing !== null} onClick={() => {
            const index = nextFitWarning(warningIndexes, reviewRef.current);
            if (index === undefined) return;
            reviewRef.current = index;
            playLine(index);
          }}>Đến câu cần nghe</button>
        </div>
      ) : null}
      <ol className="script" ref={scriptRef} onScroll={onScriptScroll}>
        {lines.map((line, index) => {
          const pending = edits.some((edit) => edit.index === index);
          const shown = picks[index]?.text ?? line.translation;
          return (
            <li
              key={`${line.start}-${index}`}
              ref={(node) => {
                rowRefs.current[index] = node;
              }}
              className={[index === spoken ? "is-current" : "", pending ? "is-pending" : ""].filter(Boolean).join(" ") || undefined}
              aria-current={index === spoken ? "true" : undefined}
            >
              {editing === index ? (
                <div className="script-edit">
                  <time>{clock(line.start)}</time>
                  {warnings[index] ? <p className="script-fit-warning edit-fit-warning">{warnings[index]}</p> : null}
                  <textarea
                    aria-label={`Sửa câu lúc ${clock(line.start)}`}
                    value={draft}
                    disabled={busy}
                    rows={3}
                    onChange={(event) => setDraft(event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === "Escape") setEditing(null);
                    }}
                  />
                  <span className="script-edit-actions">
                    <button type="button" className="link" onClick={() => setEditing(null)} disabled={busy}>
                      Huỷ
                    </button>
                    <button type="button" className="link" onClick={() => commitEdit(true)} disabled={busy}>
                      Đọc lại
                    </button>
                    <button type="button" className="check" onClick={() => commitEdit(false)} disabled={busy}>
                      Xong
                    </button>
                  </span>
                </div>
              ) : (
                <div className="script-line">
                  <button type="button" className="script-play" onClick={() => playLine(index)}>
                    <time>{clock(line.start)}</time>
                    <span>
                      <strong>{shown}</strong>
                      {warnings[index] ? <em className="script-fit-warning">{warnings[index]}{pending ? " · Chờ tạo lại để kiểm tra" : ""}</em> : null}
                      {pending ? <em>Sẽ tạo lại</em> : null}
                      {line.source ? <small>{line.source}</small> : null}
                    </span>
                  </button>
                  <button type="button" className="script-fix" onClick={() => openEdit(index)} disabled={busy}>
                    Sửa
                  </button>
                </div>
              )}
            </li>
          );
        })}
      </ol>
      {edits.length > 0 ? (
        <div className="script-retake">
          <p>
            {captionsOnly
              ? "Ghi lại chữ đã sửa lên video."
              : audioOnly
                ? "Chỉ đọc lại câu đã sửa, rồi xuất lại audio."
                : "Chỉ đọc lại câu đã sửa, rồi ghép lại video."}
          </p>
          <span>
            <button type="button" className="link" onClick={() => onPicks({})} disabled={busy}>
              Bỏ sửa
            </button>
            <button type="button" className="check" onClick={() => onRetake(edits)} disabled={busy}>
              {`Tạo lại ${edits.length} câu`}
            </button>
          </span>
        </div>
      ) : null}
    </div>
  );
}
