# Rà soát bổ sung — 31/10/2026

Phạm vi: kiểm tra lại workspace hiện tại, chạy hồi quy local, giải mã lại media nghiệm thu và đọc code điều phối nháp/cache. Không thay mã sản phẩm, không gọi dịch vụ dịch, không tải model/giọng và không chạy lại pipeline trên dữ liệu người dùng.

## Kiểm tra mới chạy

`npm run verify:local` thành công cả sáu nhóm. Bằng chứng: `.bench-asr/verify-1793426357705/report.json` và log cạnh đó.

| Nhóm | Kết quả |
| --- | --- |
| Frontend | 100 đạt, 0 lỗi |
| Hàng đợi | 3 đạt, 0 lỗi |
| Playback | 3 đạt, 0 lỗi |
| Kiểm tra đóng gói Windows | 9 đạt, 0 lỗi; không phải nghiệm thu installer trên Windows sạch |
| Build frontend | Đạt |
| Rust | 339 đạt, 0 lỗi, 4 bài bỏ qua |
| Script Whisper, chạy riêng | 11 đạt, 0 lỗi |

`git diff --check` không có lỗi khoảng trắng. Git có cảnh báo chuẩn hóa LF/CRLF cho DraftStudio.tsx, không phải lỗi build.

Giải mã lại toàn bộ chín media trong `.bench-asr/demand-20261029/audit.json` bằng FFmpeg `-xerror`: 9/9 thành công; SHA-256 cả chín file vẫn khớp audit cũ. Kết quả mới ở `.bench-asr/verify-1793426357705/media-recheck.json`. Đây là kiểm tra các file đã xuất trước, không phải chạy lại chín pipeline.

Các thay đổi đã có trong workspace gồm README, nhãn/hướng dẫn Phụ đề (không đọc) trong DraftStudio/composer và CSS. Giữ nguyên các thay đổi đó trong đợt rà soát.

## Xác nhận từ code

- Nguồn/ASR, vùng dịch, vùng nghe, lưu sửa và xuất đã có các đường riêng trong `pipeline/demand.rs`.
- Vùng tối đa 60 giây; dịch theo nhóm tối đa sáu câu, giọng từng câu. Kiểm tra ý định hiện tại giữa các nhóm/câu.
- Client giữ một tác vụ đang chạy và một yêu cầu chờ mới nhất; pause cho phép tác vụ hiện tại chốt cache, close thu hồi phiên.
- Chế độ text không yêu cầu giọng; trình phát chỉ phát WAV khi mode là voice.
- Whisper vẫn tạo WhisperModel khi chạy script; giọng WebView vẫn dùng WASM một thread. Chưa có server Whisper giữ model giữa các video.

## Điểm cần khảo sát tiếp về tốc độ

1. **Kiểm tra WAV lặp trên toàn việc khi cập nhật preview.** `draft.rs::status` đọc/băm và kiểm tra PCM của các WAV có receipt hợp lệ. `demand.rs::publish_region` gọi `view` và `clips`; `clips` và `speech_slots` lại gọi status, speech_slots còn đọc WAV lấy thời lượng. Khi đã có nhiều WAV, một cập nhật nhỏ có thể đọc lại nhiều dữ liệu của cả việc. Nên đo chi phí riêng rồi dùng snapshot đã xác minh trong cùng tác vụ, cập nhật phần thay đổi; vẫn cần kiểm tra đầy đủ ở ranh giới phù hợp, không bỏ cơ chế xác minh cache.
2. **Duyệt câu dạng bình phương.** `draft.rs::observe` gọi find trên manifest cũ cho từng câu mới; `speech_slots`/`clips` có contains trên danh sách ID sẵn sàng. Với nguồn dài nên cân nhắc map/set sau khi đo, giữ nguyên cách xác định revision và WAV hợp lệ.
3. **Mở nguồn lớn cần băm cả file.** `demand.rs::open` gọi `inspect_source`, hàm này tính SHA-256 nguồn. Đây là kiểm tra an toàn có chủ đích nhưng có thể làm mở file lớn chậm. Đo riêng trên ổ đĩa thực tế trước khi thay đổi; không đơn giản bỏ băm hoặc coi size/mtime là bằng chứng nội dung không đổi.

Đây là các chi phí xác định bằng đọc code, **chưa có benchmark mới chứng minh mức chậm hay lỗi phát thực tế**. Chưa phát hiện lỗi chức năng qua bộ kiểm tra vừa chạy.

## Giới hạn kết luận

Đã tìm thấy dữ liệu nguồn dài trong workspace: một nguồn có 2.185 cue tới giây 6.340,92; hai nguồn khác có 803 và 1.180 cue. Các folder đó chưa có draft.json, nên không coi chúng là bằng chứng nghiệm thu luồng nháp video dài. Không sửa hoặc chạy chuyển đổi các folder này.

Chưa chạy lại UI Tauri/live dịch/TTS hoặc đo RAM/VRAM/video dài trong đợt này; chưa đánh giá chất lượng dịch/phát âm bằng người. Biên bản 29/10 là bằng chứng chạy thật trước đó, tách biệt với các kiểm tra mới ở trên.

Ưu tiên tiếp: benchmark riêng chi phí cập nhật preview với hàng nghìn câu/WAV trên dữ liệu thử độc lập, sau đó mới tối ưu cache trạng thái. Chưa cần thay API hoặc thêm model để xử lý các chi phí duyệt/đọc file này.
