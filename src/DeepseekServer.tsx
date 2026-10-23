import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

// Server Deepseek-API (DeepSeek local). Màn chuẩn bị và ô "Dịch bằng" cùng dùng chỗ này.
// active: chỉ hỏi server khi đang chọn DeepSeek local. refresh: đổi giá trị thì hỏi lại ngay.
export function useDeepseekServer(active: boolean, refresh?: unknown) {
  const [up, setUp] = useState(false);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState("");

  const poll = useCallback(async () => {
    try {
      setUp(await invoke<boolean>("deepseek_server_running"));
    } catch {
      setUp(false);
    }
  }, []);

  useEffect(() => {
    if (!active) {
      setNote("");
      return;
    }
    if (busy) return;
    let alive = true;
    const ask = () => {
      invoke<boolean>("deepseek_server_running")
        .then((value) => {
          if (alive) setUp(value);
        })
        .catch(() => {
          if (alive) setUp(false);
        });
    };
    ask();
    // Server có thể tắt giữa chừng (đóng cửa sổ, lỗi), nên hỏi lại đều đều.
    const timer = window.setInterval(ask, 5000);
    return () => {
      alive = false;
      window.clearInterval(timer);
    };
  }, [active, busy, refresh]);

  const toggle = useCallback(
    async (after?: () => Promise<void> | void) => {
      setNote("");
      setBusy(true);
      try {
        if (up) {
          await invoke("stop_deepseek_server");
          setUp(false);
        } else {
          await invoke("start_deepseek_server");
          setUp(true);
        }
        await after?.();
      } catch (caught) {
        setNote(typeof caught === "string" ? caught : "Chưa bật được DeepSeek.");
        await poll();
      } finally {
        setBusy(false);
      }
    },
    [up, poll],
  );

  return { up, busy, note, toggle };
}

export function DeepseekServerRow({
  server,
  lockStop = false,
  after,
}: {
  server: ReturnType<typeof useDeepseekServer>;
  // Đang lồng thì không cho tắt, kẻo đứt giữa chừng.
  lockStop?: boolean;
  // Chạy sau khi bật hoặc tắt xong. Màn chuẩn bị dùng nó để kiểm tra lại.
  after?: () => Promise<void> | void;
}) {
  return (
    <>
      <div className="command install-row">
        <div>
          <strong>{server.up ? "Server đang chạy" : "Server chưa chạy"}</strong>
          <p>
            Bật thì chạy Deepseek-API tại http://127.0.0.1:8000. Tắt thì dừng process cổng đó. App không tự bật lại.
          </p>
        </div>
        <button
          type="button"
          onClick={() => void server.toggle(after)}
          disabled={server.busy || (server.up && lockStop)}
        >
          {server.busy ? "Đang chờ" : server.up ? "Tắt DeepSeek" : "Bật DeepSeek"}
        </button>
      </div>
      {server.note ? <p className="error install-error">{server.note}</p> : null}
    </>
  );
}
