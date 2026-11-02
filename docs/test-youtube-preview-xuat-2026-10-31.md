# Thử xuất toàn video và preview — 31/10/2026

Nguồn người dùng: YouTube `eU84kNqoGQQ`, link có `list=WL&index=7`. Dùng đúng MP4 đã tải tại `output/tai-ve/…[eU84kNqoGQQ].mp4`; không thử tải lại hay truy cập Watch Later. Nguồn dài 630,513 giây. Không gửi audio/video lên dịch vụ.

Mã: `9147476` cộng thay đổi chưa commit ở `pipeline/overlap.rs` (hàng chờ dịch/giọng). Build debug mới bằng `cargo build --offline`. Phiên app và profile WebView riêng, CDP loopback 9226; Vite localhost 1420. Giọng `minhquang`, dịch `deepseek-local` / `deepseek-chat`, tiếng Việt, thuật ngữ tắt, không đốt phụ đề, giữ khung gốc. Không đổi giọng/prefs toàn app. DeepSeek local được bật cho thử nghiệm, tắt khi xong.

Bằng chứng, script và file kết quả nằm ở `.bench-asr/youtube-eU84-20261031/` (không đưa file media vào Git).

## Xuất toàn video

Gọi `open_draft` rồi `export_draft` qua IPC thật của app, dùng bridge TTS thật. Folder `full/` ban đầu chưa có transcript, dịch hay WAV. Khi app yêu cầu kiểm tra tên, chọn giữ lời chép gốc; không tự duyệt các tên AI đề nghị. Đây là kiểm tra đường xử lý đầy đủ; không phải kiểm tra hộp thoại chọn file Windows.

- Hoàn tất sau **199,823 giây**, gồm cả chuẩn bị/chép lời mới và vòng kiểm tra tên.
- Whisper: 306 đoạn, gom thành **93 câu**.
- Tạo giọng native Piper: **64,8 giây**; co độ dài **6,7 giây**; ghép **26,2 giây**.
- Đã vượt câu 17 và tạo đủ **93 WAV**. Lượt này không tái hiện treo ở 17/93.
- Kết quả MP4: video **AV1 1920×1080**, tiếng **AAC**, độ dài video 630,521 giây / audio 630,535 giây.
- FFmpeg giải mã toàn file với `-xerror`: exit 0, không thông báo lỗi. Không còn `.partial`, `.fit.wav` hoặc `dub.wav`.
- `fit-report.json` của lượt này là danh sách rỗng. Không dùng điều này thay cho nghe nghiệm thu chất lượng.

Bằng chứng: `full-open.json`, `full-progress.json`, `full-result.json`, `full-audit.json`, `app-err.log`, script `test.mjs` và `audit.py`. Script audit ban đầu in tên Unicode ra console CP1252 gây lỗi sau khi đã ghi xong JSON; dữ liệu giải mã trong `full-audit.json` đã được đọc và kiểm tra riêng.

## Preview có giọng và tua

Folder `preview/` riêng, chỉ chép lại `transcript.json` và `lines.json` mới đo ở ca xuất; không chép bản dịch, tên hay WAV. Dùng giao diện thật qua Playwright/CDP: chọn **Giọng lồng**, **Phát**, phát đến 35 giây, tua đến 100 giây rồi phát qua 120 giây. Thu thập trạng thái HTMLVideo/HTMLAudio và thời điểm gửi/nhận Web Worker; không giả lập giọng, API hay media.

- Thực hiện đủ kịch bản trong **140,873 giây**, không có page error. Hình và audio đều tiến thời gian, audio có trạng thái đang phát trong 45 mẫu đo.
- Lần đầu chờ khoảng **40 giây** trước khi video bắt đầu chạy. Nạp model trong worker **8,562 giây**; phép đo này không bao gồm toàn bộ thời gian lấy model, dịch, kiểm tra cache và điều phối trước đó.
- **13 câu** có PCM mới; từng lần tạo tiếng mất **1,548–2,972 giây** cho khoảng 3,76–6,62 giây audio. Không có lần timeout/retry giọng trong mẫu đo.
- Dừng chờ vùng mới: khoảng **4,0 giây** gần giây 31,54; **18,6 giây** sau khi tua đến 100; **19,3 giây** gần giây 117,98. Sau đó đều phát tiếp.
- Khoảng chờ không đồng nghĩa worker đang tính toàn bộ thời gian: trace có những khoảng không có yêu cầu `speak` đang chạy. Chưa có phép đo từng công đoạn đủ để quy hết phần chờ cho dịch, IPC hoặc xác minh cache.
- Phát lại vùng đầu đã cache: trong **3,323 giây** kể từ thao tác, video đã chạy tới 2,007 giây, audio đang phát; **không có yêu cầu worker mới**.

Bằng chứng: `preview-result.json`, `preview-summary.json`, `preview-seek.json`, `replay-result.json`, `preview-playing.png`, `preview-final.png`; script `preview.mjs`, `replay.mjs`. Preview chỉ nghe/xem các vùng nêu trên, chưa phát liên tục hết 10 phút 30 giây và chưa nghe đánh giá phát âm bằng người.

## Điểm cần sửa/đo tiếp

1. `finish_edited` chưa đánh dấu bước dịch hoàn tất: snapshot vẫn hiện `translate: running` / “Dịch lại 5 câu” khi đã đọc giọng và ghép. Trạng thái này gây hiểu nhầm bị kẹt; ca xuất vẫn hoàn tất.
2. Preview phát được nhưng độ trễ chuẩn bị vùng mới còn cao. Thời gian tạo PCM riêng không giải thích hết độ trễ. Cần đo thêm thời gian lấy model, chuẩn bị vùng, xác minh nháp và chờ dịch trước khi khẳng định nguyên nhân.
3. Lượt này chưa tái hiện treo giọng vô hạn hoặc đứng cố định ở 17/93. Không coi kết quả này là đã chữa tận gốc báo lỗi ban đầu.
