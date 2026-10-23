# Lồng tiếng video trên máy

App desktop để chọn một video trên máy, tách tiếng, chép lời, dịch, đọc lại bằng giọng tiếng Việt, rồi ghi video mới vào folder local. Không gửi audio hay video đi. Tên app chưa chốt.

## Luồng

1. FFmpeg tách tiếng.
2. Whisper chép lời.
3. Dịch. Có API key Gemini, ChatGPT, Claude hoặc DeepSeek thì dùng luôn (chỉ chữ lời thoại đi qua mạng). Ollama không bắt buộc, chỉ cài nếu muốn dịch trên máy: model `qwen3.5:9b-q4_K_M` hoặc `gemma3:12b` tại `http://localhost:11434`.
4. Giọng Piper đọc tiếng Việt bằng file giọng trong `voices/vi`. Có `piper.exe` trong folder `piper` thì app đọc bằng chương trình đó. Không có thì giọng chạy ngay trong cửa sổ app (ONNX Runtime Web).
5. FFmpeg ghép video, phụ đề và tiếng lồng vào folder người dùng chọn.

## Hiện có

- Màn chuẩn bị: kiểm tra FFmpeg, Ollama (chỉ cần khi dịch trên máy), model dịch, giọng, Whisper. Mục thiếu thì hiện cách tải. App không tự tải gì, trừ nút "Tải Whisper".
- `voices/<mã ngôn ngữ>` (`vi`, `en`, `zh`, `ja`, `ko`, `fr`, `de`, `es`, `ru`, `th`, `id`, `pt`): các cặp `.onnx` và `.onnx.json`. Giọng được đọc bằng `onnxruntime-web` và `phonemizer` (espeak-ng WASM) trong Web Worker, kèm bộ chuẩn hóa tiếng Việt (số, ngày, tiền, từ viết tắt) lấy từ project Local-TTS (`src/tts`). Một luồng, khoảng 1,8 lần thời gian thực.
- Chọn giọng và nghe thử: mục "Giọng Việt" trên màn chuẩn bị có ô chọn giọng (lưu lại cho lần sau) và nút Thử giọng đọc một câu mẫu.
- Khi đọc giọng, Rust gửi danh sách câu cho cửa sổ app, cửa sổ đọc rồi gửi wav về để ghi vào folder việc. Phải giữ cửa sổ app mở khi đang lồng.
- Màn lồng tiếng: chọn video và folder kết quả, bấm Lồng tiếng hoặc Dừng, xem trạng thái từng bước.
- Pipeline (`src-tauri/src/pipeline`): tách tiếng, chép lời, ngắt câu, sửa tên (lần đầu dừng để sửa tên bị chép sai; phụ đề và giọng dùng tên đó), lấy thuật ngữ, dịch (theo cụm 10 câu, có ngữ cảnh), đọc giọng (đọc cả lô câu một lần nạp giọng; câu quá dài được rút gọn rồi đọc lại), co độ dài (xếp câu nối tiếp, mượn khoảng trống tối đa 1,5 giây, tăng tốc tối đa 1,4 lần, câu vẫn không vừa thì cắt), ghép video.
Mỗi video có một folder việc trong folder kết quả, đặt theo tên video:

- `audio.wav`, `transcript.json`, `lines.json`, `names.json`, `terms.json`, `translation.json`
- `lines/001.wav`, `lines/002.wav`, ... tiếng đọc từng câu; `lines/001.fit.wav`, ... bản đã co độ dài
- `dub.wav` tiếng lồng cả video; `fit-report.json` liệt kê câu bị cắt bớt
- `<tên video>.vi.mp4` và `<tên video>.vi.srt` kết quả

Folder kết quả mặc định là `output/` ở gốc project (app tự tạo, đổi được bằng nút "Đổi folder"). Ví dụ `output/tap 1/tap 1.vi.mp4`.

Bước nào đã có file thì lần chạy sau bỏ qua. Bản dịch lưu sau từng cụm nên dừng giữa chừng vẫn dịch tiếp được. Muốn làm lại bước nào thì xóa file của bước đó.

Whisper: mục Whisper ở màn chuẩn bị có nút **Tải Whisper**. Một cú bấm: app kiểm tra bản whisper.cpp mới nhất trên GitHub và model trên Hugging Face, chỉ tải phần còn thiếu hoặc cũ hơn (khoảng 475 MB lần đầu), đối chiếu mã băm SHA-256 rồi tự giải nén vào folder `whisper` trong project. Đã đủ và mới nhất thì không tải lại. Tải tay vẫn được: giải nén `whisper-bin-x64.zip` vào folder `whisper` và chép `ggml-small.bin` (hoặc base, medium, large-v3, tiny) vào cùng folder. Nếu `whisper-cli` đã có trong PATH thì dùng luôn, model nằm cạnh nó. Màn chuẩn bị báo rõ thiếu file nào.

Chưa chạy thử trọn luồng với video thật.

## Chạy

```powershell
npm install
npm run tauri dev
```

`npm run tauri dev` mở cửa sổ app. `npm run build` build bản cài.
