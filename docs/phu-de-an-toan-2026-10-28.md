# Mốc A — Chia phụ đề an toàn

Ngày: 28/10/2026. Giữ Tauri, không thay dữ liệu lời chép, bản dịch hay WAV của việc cũ; không gọi dịch vụ hoặc tải model. Chưa triển khai timeline, chỉnh mốc hoặc tách/gộp câu.

## Thay đổi

- Bố cục chữ dùng các đoạn có mốc và tối đa hai dòng. Dòng là lát cắt liên tục của nội dung đã chuẩn hóa khoảng trắng; kiểm tra ghép lại phải khớp đầy đủ trước khi tạo ASS.
- Tách theo cụm ký tự Unicode, không cắt rời dấu tổ hợp, emoji gia đình, cờ hoặc màu da. Ưu tiên ranh giới từ và dấu câu gần cuối dòng; từ/URL dài vượt khung vẫn có thể phải chia qua dòng/đoạn, không coi đó là ngắt ngữ nghĩa hoàn hảo.
- Cấp ít nhất một phần trăm giây cho từng đoạn ASS và làm tròn mốc vào trong khoảng câu gốc. Không đủ thời gian để giữ tất cả đoạn thì báo câu lỗi thay vì bỏ chữ. Mốc sai hoặc block không đọc được cũng báo lỗi, không âm thầm bỏ qua như trước.
- Ký tự ngoặc nhọn và dấu gạch chéo ngược được đưa vào ASS an toàn, giữ hình dạng chữ thay vì đổi thành ký tự fullwidth. Gạch chéo ngược thêm WORD JOINER vô hình để tránh bị hiểu thành lệnh xuống dòng/khoảng trắng. Dữ liệu SRT gốc không thay đổi.
- Câu cha chồng thời gian vẫn được giữ khi xuất, không tự sửa mốc. Màn lời thoại của video `.chu.mp4` đánh dấu cả hai câu chồng nhau, thời gian sai, thời gian hiện dưới 0,65 giây hoặc mật độ trên 25 cụm ký tự không trắng/giây. Hai ngưỡng cuối chỉ là gợi ý, không chặn xuất và không phải kết luận chất lượng cho mọi ngôn ngữ.
- Cảnh báo mới dùng nút Đến câu cần kiểm tra có sẵn. Audio và video không ghi chữ không nhận cảnh báo độ dễ đọc. Bổ sung chặn seek tới mốc không hữu hạn/âm.
- Các cảnh báo được tính lại từ câu hiện có khi đọc kết quả, không có cache báo cáo mới có thể bị cũ. Đoạn thử và xuất đầy đủ tiếp tục dùng chung đường chia chữ. Bản xuất cũ không tự bị tạo lại chỉ vì nâng phiên bản app.

## Kiểm chứng

`npm run verify:local` qua cả năm nhóm: 82 kiểm thử frontend, 3 kiểm thử hàng đợi, 9 kiểm thử kiểm tra gói Windows, build TypeScript/Vite và 254 kiểm thử Rust qua (3 bỏ qua theo cấu hình có sẵn). Báo cáo: `.bench-asr/verify-1793178443297/report.json`.

Sau lượt trên, bổ sung nhánh dùng video thật cho phép thử FFmpeg và sửa kiểm tra giao nhau khi file câu không theo thứ tự. Đã chạy lại toàn bộ Rust: 254 qua, 0 lỗi, 3 bỏ qua. Không thay frontend sau lượt verify.

Kiểm thử mới bao gồm:

- Tiếng Việt có dấu tổ hợp; Trung/Nhật/Hàn; emoji, cờ, tên Latin, số tiền, ngày và URL; khoảng trắng nhiều loại. Kiểm tra lát cắt nguyên vẹn ở nhiều độ rộng.
- Câu cực ngắn, NaN/Infinity, mốc âm/ngược, phút sai, phần lẻ sai, block thiếu thời gian và cue chồng nhau. Không đủ một tick cho từng đoạn thì lỗi rõ, không mất đoạn.
- Cảnh báo hai phía của câu chồng nhau, không báo chồng giả khi file không xếp theo thời gian; không ghi file khi đọc cảnh báo và không gắn cảnh báo phụ đề vào audio.
- FFmpeg dựng ở 9:16, 16:9, 1:1. Phụ đề lỗi không làm thay đổi byte của bản xuất cũ, video nguồn hoặc SRT hợp lệ; không để lại ASS/partial. Bộ hồi quy còn kiểm tra dừng, ghép, bản có chữ, nguồn xoay và dựng đoạn thử.

Đã chạy thêm phép thử FFmpeg bằng 6 giây đầu của `output/kiem-thu-20261028/hometown-75s.mp4`, với lời Việt kiểm thử riêng. Không chép lời/dịch/đọc giọng lại và không sửa nguồn. Cả ba tỉ lệ dựng thành công; đã xem ba ảnh khung hình: chữ Việt đủ dấu, tối đa hai dòng và nằm trong khung ở mốc đã xem. Mẫu này không xác nhận chất lượng ngắt nghĩa của mọi câu hoặc chất lượng toàn bộ video 75 giây.

Mẫu local được giữ ở `.bench-asr/caption-review-20261028/` gồm ba PNG, ba MP4 và SRT thử. Ảnh ngang còn chữ tiếng Anh đã có sẵn trong nguồn chồng với vùng chữ Việt; mốc A không dò/xóa chữ nguồn. Người dùng vẫn dùng Che/Làm mờ hoặc đổi vị trí chữ khi cần.

Chạy lại kiểm tra hình bằng nguồn local tùy chọn:

```powershell
$env:DUB_CAPTION_REVIEW = '1'
$env:DUB_CAPTION_REVIEW_SOURCE = (Resolve-Path 'output/kiem-thu-20261028/hometown-75s.mp4').Path
cargo test --manifest-path src-tauri/Cargo.toml --offline --lib pipeline::assemble::tests::adaptive_caption_burn_preserves_aspect_and_cleans_overlay -- --nocapture
Remove-Item Env:DUB_CAPTION_REVIEW, Env:DUB_CAPTION_REVIEW_SOURCE
```

## Giới hạn

Độ rộng chữ vẫn là ước lượng, không đo pixel từng font. Cảnh báo mật độ dùng thời gian câu cha, không phải căn từ theo giọng hoặc báo cáo từng đoạn hiển thị. File tiếng/SRT cũ vẫn đọc theo định dạng cũ; không tự di trú dữ liệu.

Chưa kiểm tra thao tác bấm cảnh báo mới trong cửa sổ Tauri, chưa nghiệm thu mọi font/ngôn ngữ và chưa làm lại bản đóng gói Windows. Kiểm thử này không xác nhận thêm chất lượng Whisper, dịch, giọng hoặc installer. Chưa commit.
