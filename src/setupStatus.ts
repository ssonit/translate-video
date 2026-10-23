import { OPTIONAL_TOOLS, guideFor, type ToolId } from "./guides.ts";

export type ToolStatus = {
  id: ToolId;
  ready: boolean;
  detail: string;
};

// checking: chưa có kết quả nào và đang kiểm tra. failed: không có kết quả và không còn kiểm tra
// (chạy bằng trang Vite một mình, hoặc lệnh lỗi). gap: thiếu ít nhất một mục bắt buộc.
export type SetupSummary = {
  kind: "checking" | "failed" | "ready" | "gap";
  missing: ToolId[];
};

const isRequired = (tool: ToolStatus) => !OPTIONAL_TOOLS.includes(tool.id);

// Kiểm tra lại thì vẫn giữ kết quả lần trước, để chip không nháy về "Đang kiểm tra".
export function summarize(tools: ToolStatus[], checking: boolean): SetupSummary {
  const required = tools.filter(isRequired);
  if (required.length === 0) return { kind: checking ? "checking" : "failed", missing: [] };
  const missing = required.filter((tool) => !tool.ready).map((tool) => tool.id);
  return { kind: missing.length > 0 ? "gap" : "ready", missing };
}

export function chipLabel(summary: SetupSummary): string {
  switch (summary.kind) {
    case "checking":
      return "Đang kiểm tra";
    case "failed":
      return "Chưa kiểm tra được";
    case "ready":
      return "Đủ đồ nghề";
    case "gap":
      return summary.missing.length === 1
        ? `Thiếu ${guideFor(summary.missing[0]).name}, xem`
        : `Thiếu ${summary.missing.length} mục, xem`;
  }
}

// Dòng báo cạnh nút Chạy. Trống nghĩa là không khóa: đang kiểm tra hay không đọc được thì vẫn cho
// chạy, vì Rust báo lỗi rõ ngay từ đầu khi thiếu thật.
export function lockNote(summary: SetupSummary): string {
  if (summary.kind !== "gap") return "";
  return `Chưa chạy được: thiếu ${summary.missing.map((id) => guideFor(id).name).join(", ")}.`;
}

export function firstGap(tools: ToolStatus[]): ToolId | undefined {
  return tools.find((tool) => isRequired(tool) && !tool.ready)?.id;
}

// Nhãn ở mỗi dòng của màn Đồ nghề. Chưa có kết quả thì không được gọi là "Thiếu".
export function markFor(
  status: ToolStatus | undefined,
  optional: boolean,
  checking: boolean,
): { text: string; tone: "ready" | "bad" | "wait" } {
  if (!status) return { text: checking ? "Đang kiểm tra" : "Chưa rõ", tone: "wait" };
  if (status.ready) return { text: "Có", tone: "ready" };
  return { text: optional ? "Tùy chọn" : "Thiếu", tone: "bad" };
}
