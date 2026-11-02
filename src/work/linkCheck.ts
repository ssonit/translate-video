import { invoke } from "@tauri-apps/api/core";
import { errorText } from "./format";

// Hỏi Rust xem link có dùng được không, theo đúng luật lúc tải. Trả lý do; chuỗi trống là dùng được.
export async function linkProblem(link: string) {
  try {
    await invoke("check_link", { url: link });
    return "";
  } catch (caught) {
    return errorText(caught);
  }
}
