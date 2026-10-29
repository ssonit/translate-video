import type { View } from "./types";

const destinations: { view: View; label: string; path: string }[] = [
  { view: "new", label: "Lồng tiếng", path: "m8 5 11 7-11 7z" },
  { view: "progress", label: "Hàng đợi", path: "M9 6h11M9 12h11M9 18h11M4 6h.01M4 12h.01M4 18h.01" },
  { view: "library", label: "Thư viện", path: "M3 7h7l2-3h9v16H3z" },
  { view: "presets", label: "Bộ cài đặt", path: "M4 7h16M4 17h16M8 4v6M16 14v6" },
  { view: "settings", label: "Cài đặt", path: "M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8M12 2v3M12 19v3M2 12h3M19 12h3M5 5l2 2M17 17l2 2M5 19l2-2M17 7l2-2" },
  { view: "help", label: "Hướng dẫn", path: "M4 4h6l2 2 2-2h6v15h-6l-2 2-2-2H4zM12 6v15M7 8h2M7 12h2M15 8h2M15 12h2" },
];

export function StudioNav({ view, count, running, detail, result, preview, previewLabel, onView }: {
  view: View; count: number; running: boolean; detail: string; result: boolean;
  preview: "live" | "parts" | null; previewLabel: string; onView: (view: View) => void;
}) {
  return <nav className="studio-nav" aria-label="Điều hướng chính">
    <div className="studio-nav-mark" aria-hidden="true"><i /><i /><i /><i /><i /></div>
    <div className="studio-nav-items">{destinations.map(item => <button key={item.view} type="button"
      className={view === item.view || item.view === "library" && ["watch", "resume"].includes(view) ? "studio-nav-item is-active" : "studio-nav-item"}
      aria-current={view === item.view ? "page" : undefined} title={item.label} onClick={() => onView(item.view)}>
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d={item.path} /></svg>
      <span>{item.label}</span>{item.view === "progress" && count > 0 ? <small>{count}</small> : null}
    </button>)}</div>
    {result ? <button type="button" className="studio-return" onClick={() => onView("watch")} title="Video đang mở">↗ <span>Video đang mở</span></button> : null}
    {preview ? <button type="button" className="studio-return" title={previewLabel} onClick={() => onView(preview)}>▷ <span>{previewLabel}</span></button> : null}
    <div className="studio-nav-bottom">
      {running ? <button type="button" className="studio-running" onClick={() => onView("progress")}><i /><span>{detail}</span></button> : <p>Xử lý video trên máy</p>}
    </div>
  </nav>;
}
