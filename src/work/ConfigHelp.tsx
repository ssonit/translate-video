import { useEffect, useId, useRef, useState } from "react";

export function ConfigHelp() {
  const id = useId();
  const root = useRef<HTMLSpanElement>(null);
  const [open, setOpen] = useState(false);
  const [pinned, setPinned] = useState(false);

  useEffect(() => {
    if (!open) return;
    const close = () => { setOpen(false); setPinned(false); };
    const outside = (event: PointerEvent) => {
      if (event.target instanceof Node && !root.current?.contains(event.target)) close();
    };
    const escape = (event: KeyboardEvent) => { if (event.key === "Escape") close(); };
    document.addEventListener("pointerdown", outside);
    document.addEventListener("keydown", escape);
    return () => {
      document.removeEventListener("pointerdown", outside);
      document.removeEventListener("keydown", escape);
    };
  }, [open]);

  return <span className="studio-config-help" ref={root}
    onMouseEnter={() => setOpen(true)} onMouseLeave={() => { if (!pinned) setOpen(false); }}>
    <button type="button" className="studio-info-button" aria-label="Tìm hiểu về hình ảnh, chữ và âm thanh"
      aria-expanded={open} aria-controls={id} aria-describedby={open ? id : undefined}
      onFocus={() => setOpen(true)} onBlur={() => { setOpen(false); setPinned(false); }}
      onClick={() => { setPinned(!pinned); setOpen(!pinned); }}>ⓘ</button>
    <span id={id} role="tooltip" className="studio-config-tooltip" hidden={!open}>
      Bạn có thể thêm logo, che chữ cũ và nghe thử cách đọc tên tại đây. Các thay đổi áp dụng khi xuất video.
    </span>
  </span>;
}
