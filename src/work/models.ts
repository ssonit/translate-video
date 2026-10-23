import { LOCAL_MODELS } from "../guides";

// Gợi ý model cho từng dịch vụ; ô nhập vẫn cho gõ tên model khác. Trống = model mặc định của app.
export const OTHER_MODEL = "__other__";

export const MODEL_HINTS: Record<string, { name: string; fallback: string; models: string[] }> = {
  local: { name: "Ollama trên máy", fallback: "qwen3.5:9b-q4_K_M", models: LOCAL_MODELS.map((item) => item.name) },
  gemini: {
    name: "Gemini",
    fallback: "gemini-3.8-flash",
    models: ["gemini-3.8-flash", "gemini-2.5-flash", "gemini-2.5-flash-lite", "gemini-2.5-pro"],
  },
  openai: { name: "ChatGPT", fallback: "gpt-4o-mini", models: ["gpt-4o-mini", "gpt-4o", "gpt-4.1-mini"] },
  claude: {
    name: "Claude",
    fallback: "claude-haiku-4-5-20251001",
    models: ["claude-haiku-4-5-20251001", "claude-sonnet-5-5", "claude-opus-5-5"],
  },
  deepseek: {
    name: "DeepSeek",
    fallback: "deepseek-flash",
    models: ["deepseek-flash", "deepseek-v4-pro"],
  },
};
