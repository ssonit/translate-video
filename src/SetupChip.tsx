import { chipLabel, type SetupSummary } from "./setupStatus";

// Kết quả kiểm tra máy, một dòng ở đầu màn lồng tiếng. Bấm vào mở màn Đồ nghề.
export function SetupChip({ summary, onClick }: { summary: SetupSummary; onClick: () => void }) {
  return (
    <button
      type="button"
      className={`setup-chip is-${summary.kind}`}
      title="Mở Đồ nghề: kiểm tra máy, chọn giọng, chọn dịch vụ dịch"
      onClick={onClick}
    >
      {chipLabel(summary)}
    </button>
  );
}

// Dòng báo cạnh nút Chạy khi thiếu mục bắt buộc, kèm đường sang màn Đồ nghề. Trống thì không vẽ gì.
export function LockedNote({ note, onTools }: { note: string; onTools: () => void }) {
  if (!note) return null;
  return (
    <>
      <p className="foot-note">{note}</p>
      <button type="button" className="link" onClick={onTools}>
        Xem đồ nghề
      </button>
    </>
  );
}
