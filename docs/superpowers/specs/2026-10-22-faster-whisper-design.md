# Chép lời bằng faster-whisper trên GPU

## Mục tiêu

Bước chép lời và xem trước dùng faster-whisper trên GPU khi máy có đủ đồ. Video khoảng 22 phút trên RTX 4060 không còn chạy Whisper CPU 2 luồng. Thiếu đồ thì vẫn gọi `whisper-cli` như hiện tại.

## Cách chạy

Rust không nhúng model. Khi đường nhanh sẵn sàng, `transcribe` (`src-tauri/src/pipeline/media.rs`) và `stream_whisper` (`src-tauri/src/watch.rs`) cùng gọi một lệnh:

`py -3 -u scripts/faster_whisper.py --model <folder> --audio <audio.wav tuyệt đối> --out <transcript.json tuyệt đối> --duration <giây>`

`python` thay `py -3` khi máy không có launcher `py`. `-u` để dòng in ra không bị đệm, xem trước nhận câu trong lúc chạy.

Script chỉ nhận folder model trên đĩa. Không truyền tên model `large-v3-turbo` cho thư viện, vì tên đó khiến faster-whisper tải từ Hugging Face trong lúc chép. Audio không rời máy.

Model: folder `whisper/large-v3-turbo/` có file `model.bin` (bản CTranslate2 của `Systran/faster-whisper-large-v3-turbo`). GPU: `device="cuda"`, `compute_type="float16"`. Nhận ngôn ngữ tự động (`language=None`), `beam_size=5`, `vad_filter=False`, `word_timestamps=False`. Một worker.

## Đường nào được chọn

Đường nhanh khi cả ba đúng:

- `scripts/faster_whisper.py` tồn tại
- `whisper/large-v3-turbo/model.bin` tồn tại
- `py -3` hoặc `python` chạy được đoạn import `faster_whisper` và `ctranslate2.get_cuda_device_count() > 0`

Có đường nhanh thì dùng, kể cả khi không có `whisper-cli`. Không có thì lệnh cũ: `whisper-cli -m <ggml> -f audio.wav -t 2 -l auto -oj -pp -of transcript`, thư mục làm việc là folder việc (tránh lỗi ANSI của whisper-cli). Không có cả hai thì câu lỗi giữ nguyên: `Chưa thấy whisper-cli.exe. Xem mục Whisper ở màn chuẩn bị.` Có exe mà thiếu ggml thì câu lỗi model cũ giữ nguyên.

Lệnh GPU thất bại giữa chừng thì dừng và báo lỗi. Không tự chạy lại bằng whisper-cli trong cùng lượt.

Đã có `transcript.json` có câu thì bỏ qua, như hiện tại.

## Hợp đồng in ra

Mỗi câu, một dòng stdout, đúng dạng xem trước đã parse:

`[00:00:01.000 --> 00:00:04.000]  Xin chào`

Hai dấu cách sau `]`. Giờ `HH:MM:SS.mmm`. Bỏ câu chữ rỗng. `end` nhỏ hơn `start + 0.05` thì nâng lên mức đó.

Tiến độ trên stderr, có xuống dòng: `progress = 13%`. Phần trăm là số nguyên, làm tròn, kẹp 0..100, tính `end / duration`. Hết việc thì `progress = 100%`.

Xong, ghi `transcript.json` là mảng `[{"start": <giây>, "end": <giây>, "text": "..."}]`, UTF-8, không escape chữ Unicode. `read_cues` đã đọc dạng này.

Không nghe được câu nào: không tạo `transcript.json` (file rỗng sẽ chặn lần chép sau), in stderr đúng câu `Không nghe được lời thoại.` rồi thoát mã khác 0.

Thiếu `model.bin`: không gọi `WhisperModel`, in stderr `Chưa thấy model large-v3-turbo trong folder whisper.`

Lỗi GPU hoặc thư viện: in stderr `faster-whisper không chạy được trên GPU.`

## Màn chuẩn bị

Vẫn một mục `whisper`. Không thêm `ToolId`, không nút tải mới. Nút "Tải Whisper" giữ cho đường `whisper-cli`.

Sẵn sàng khi có đường nhanh hoặc có cả `whisper-cli` và file ggml. Có đường nhanh thì dòng trạng thái đúng câu: `Đã thấy faster-whisper trên GPU và model large-v3-turbo.`

Hướng dẫn mục Whisper nói rõ cách cài Python 3.11 trở lên, lệnh `py -3 -m pip install "faster-whisper>=1.1.0"`, và chép file model vào `whisper\large-v3-turbo`. App không tự tải model GPU.

## Ngoài phạm vi

Không đổi ngắt câu, dịch, đọc giọng, ghép. Không gửi audio đi. Không đổi tên app. Không xóa whisper.cpp.
