import { invoke } from "@tauri-apps/api/core";

// Keep the request pending until Rust releases its task; cancellation must not
// let another operation start while synthesis is still stopping.
export async function playVoicePreview(lang: string, name: string, signal: AbortSignal, text?: string) {
  if (signal.aborted) return;
  let synthesizing = true;
  const cancel = () => {
    if (synthesizing) void invoke("cancel_dub").catch(() => {});
  };
  signal.addEventListener("abort", cancel, { once: true });
  try {
    const bytes = await invoke<ArrayBuffer>("preview_voice", { lang, name, text });
    synthesizing = false;
    if (signal.aborted) return;
    const url = URL.createObjectURL(new Blob([bytes], { type: "audio/wav" }));
    const audio = new Audio(url);
    try {
      await new Promise<void>((resolve, reject) => {
        const finish = () => { signal.removeEventListener("abort", finish); audio.pause(); resolve(); };
        signal.addEventListener("abort", finish, { once: true });
        audio.onended = finish;
        audio.onerror = () => { signal.removeEventListener("abort", finish); reject(new Error("Không phát được câu thử.")); };
        void audio.play().catch(error => { signal.removeEventListener("abort", finish); reject(error); });
      });
    } finally {
      audio.pause();
      audio.removeAttribute("src");
      URL.revokeObjectURL(url);
    }
  } catch (error) {
    if (!signal.aborted) throw error;
  } finally {
    synthesizing = false;
    signal.removeEventListener("abort", cancel);
  }
}
