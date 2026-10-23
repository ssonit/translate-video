export type ToolId = "ffmpeg" | "ollama" | "model" | "voice" | "whisper" | "ytdlp" | "piper";

// Missing optional tools never block the dubbing screen.
export const OPTIONAL_TOOLS: ToolId[] = ["ytdlp", "piper"];

export type GuideLink = {
  label: string;
  url: string;
};

export type CloudChoice = {
  id: "gemini" | "openai" | "claude" | "deepseek";
  name: string;
  keyUrl: string;
};

export type LocalModel = {
  name: string;
  label: string;
  // Câu chọn model theo máy, hiện trên thẻ và cạnh ô chọn ở màn lồng tiếng.
  when: string;
  note: string;
  command: string;
};

// Danh sách gợi ý khi dịch trên máy. Model đã kéo thì hiện "Đã có"; model chưa có thì hiện lệnh tải.
export const LOCAL_MODELS: LocalModel[] = [
  {
    name: "translategemma:4b",
    label: "TranslateGemma 4B",
    when: "Dùng khi máy nhẹ: card khoảng 4 đến 6 GB, hoặc card 8 GB mà muốn chừa chỗ.",
    note: "Nhẹ nhất, chuyên dịch. File khoảng 3,3 GB.",
    command: "ollama pull translategemma:4b",
  },
  {
    name: "qwen3.5:9b-q4_K_M",
    label: "Qwen 3.5 9B",
    when: "Dùng khi card khoảng 8 GB. Nằm trọn trên card, là model mặc định.",
    note: "File khoảng 6,6 GB.",
    command: "ollama pull qwen3.5:9b-q4_K_M",
  },
  {
    name: "gemma3:12b",
    label: "Gemma 3 12B",
    when: "Dùng khi card từ 12 GB. Dịch mượt hơn. Card 8 GB sẽ tràn sang RAM và chậm.",
    note: "File khoảng 8,2 GB.",
    command: "ollama pull gemma3:12b",
  },
];

export type Guide = {
  id: ToolId;
  name: string;
  role: string;
  steps: string[];
  links: GuideLink[];
  command?: string;
  commandName?: string;
  commandNote?: string;
  clouds?: CloudChoice[];
};

export const GUIDES: Guide[] = [
  {
    id: "ffmpeg",
    name: "FFmpeg",
    role: "Tách tiếng và ghép video",
    steps: [
      "Mở trang bản dựng Windows bên cạnh.",
      "Tải ffmpeg-release-essentials.zip.",
      "Giải nén. Vào thư mục bin, copy đường dẫn thư mục đó.",
      "Tìm Environment Variables trên Windows, mở Path, thêm đường dẫn bin.",
      "Đóng terminal cũ. Mở terminal mới và gõ ffmpeg -version.",
      "Quay lại đây và bấm Kiểm tra lại.",
    ],
    links: [{ label: "Mở trang tải FFmpeg", url: "https://www.gyan.dev/ffmpeg/builds/" }],
  },
  {
    id: "ollama",
    name: "Ollama",
    role: "Chạy model dịch trên máy",
    steps: [
      "Mở trang tải Ollama.",
      "Tải OllamaSetup.exe cho Windows và chạy file đó.",
      "Cài xong, mở Ollama từ Start menu.",
      "Quay lại đây và bấm Kiểm tra lại.",
    ],
    links: [{ label: "Mở trang tải Ollama", url: "https://ollama.com/download" }],
  },
  {
    id: "model",
    name: "Model dịch",
    role: "Một model trên máy, hoặc dịch vụ qua mạng",
    steps: [
      "Trên máy: ba model dưới là gợi ý. App chỉ kiểm tra ba model đó. Muốn model khác thì sang màn lồng tiếng, chọn Khác và gõ tên. Model chưa có thì chép lệnh rồi dán vào PowerShell.",
      "Qua mạng: chọn Gemini, ChatGPT, Claude hoặc DeepSeek, dán API key rồi lưu.",
      "Bấm Kiểm tra lại.",
    ],
    links: [],
    clouds: [
      { id: "gemini", name: "Gemini", keyUrl: "https://aistudio.google.com/apikey" },
      { id: "openai", name: "ChatGPT", keyUrl: "https://platform.openai.com/api-keys" },
      { id: "claude", name: "Claude", keyUrl: "https://console.anthropic.com/settings/keys" },
      { id: "deepseek", name: "DeepSeek", keyUrl: "https://platform.deepseek.com/api_keys" },
    ],
  },
  {
    id: "voice",
    name: "Giọng Việt",
    role: "Đọc lời tiếng Việt trên máy",
    steps: [
      "Giọng nằm trong folder voices\\vi của project này, mỗi giọng là một cặp file .onnx và .onnx.json.",
      "Có piper.exe trong folder piper thì app đọc bằng chương trình đó. Không có thì app vẫn tự đọc các file giọng này.",
      "Nếu thiếu, chép cặp file giọng vào voices\\vi rồi bấm Kiểm tra lại. App không tự tải giọng.",
    ],
    links: [],
  },
  {
    id: "whisper",
    name: "Whisper",
    role: "Chép lời nói trong video thành chữ",
    steps: [
      "Muốn chép nhanh trên GPU: cài Python 3.11 trở lên. Trong PowerShell chạy py -3 -m pip install \"faster-whisper>=1.1.0\". Tải model ở link bên dưới, chép các file vào whisper\\large-v3-turbo sao cho có model.bin. App không tự tải model GPU.",
      "Bấm Kiểm tra lại. Thấy GPU thì app dùng đường này. Không thấy thì dùng whisper-cli ở bước dưới.",
      "Bấm nút \"Tải Whisper\" bên dưới. App kiểm tra bản mới nhất rồi tự tải, kiểm tra file và giải nén vào folder whisper. Việc này mất vài phút vì model khoảng 466 MB.",
      "Chờ thanh tiến độ chạy xong. App tự kiểm tra lại.",
      "Nếu mạng chặn việc tải, dùng hai nút \"Tải tay\" bên dưới: giải nén file zip vào folder whisper (cạnh folder voices) và chép ggml-small.bin vào cùng folder.",
    ],
    links: [
      {
        label: "Tải tay: model GPU",
        url: "https://huggingface.co/Systran/faster-whisper-large-v3-turbo",
      },
      {
        label: "Tải tay: chương trình",
        url: "https://github.com/ggml-org/whisper.cpp/releases/download/b5454/whisper-bin-x64.zip",
      },
      {
        label: "Tải tay: model",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin",
      },
    ],
  },
  {
    id: "ytdlp",
    name: "yt-dlp",
    role: "Tải video từ link YouTube và TikTok",
    steps: [
      "Tải yt-dlp cho Windows và thêm vào Path. Chọn file trên máy thì không cần.",
      "Link Douyin không dùng yt-dlp. App tự mở Edge. Link TikTok dùng yt-dlp.",
      "Bấm Kiểm tra lại.",
    ],
    links: [{ label: "Mở trang yt-dlp", url: "https://github.com/yt-dlp/yt-dlp/releases" }],
  },
  {
    id: "piper",
    name: "Piper",
    role: "Đọc giọng nhanh hơn, trên máy",
    steps: [
      "Tải piper_windows_amd64.zip ở trang Releases. Giải nén cả thư mục, để piper.exe và espeak-ng-data cạnh nhau.",
      "Chép cả thư mục đó thành folder piper cạnh folder voices của project này. Không cần thêm vào Path.",
      "Bấm Kiểm tra lại. Thiếu thì app vẫn đọc giọng trong cửa sổ.",
    ],
    links: [{ label: "Mở trang Piper", url: "https://github.com/rhasspy/piper/releases" }],
  },
];

export function guideFor(id: ToolId): Guide {
  return GUIDES.find((guide) => guide.id === id) ?? GUIDES[0];
}
