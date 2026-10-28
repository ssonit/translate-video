# Bổ sung kiểm thử phát hành ngày 28/10/2026

Tiếp tục mục 7 của kế hoạch nâng cấp, trên máy phát triển hiện tại. Không tải công cụ/model, dùng API key, chạy installer hoặc thay cấu hình người dùng.

## Thay đổi

- Link YouTube được kiểm tra là một video trước khi gọi yt-dlp hoặc hỏi tiêu đề. Nhận watch, youtu.be, shorts, live và embed với ID hợp lệ; từ chối playlist, kênh, link thiếu/sai ID và đường dẫn thừa. Link watch có cả ID video và tham số list vẫn tải riêng video đó bằng `--no-playlist`. Đoạn chữ chia sẻ YouTube vẫn bị từ chối; Douyin/TikTok giữ cách lấy URL trong đoạn chia sẻ.
- Script kiểm tra gói yêu cầu đủ EXE, hướng dẫn, script kiểm tra, faster-whisper và toàn bộ module Douyin. File có mặt nhưng không được kê trong manifest cũng làm kiểm tra thất bại; không còn trường hợp manifest kê thiếu vẫn báo đạt. Từ chối SHA-256 hoặc kích thước không hợp lệ.
- `npm run test:release` kiểm tra 9 tình huống bằng Windows PowerShell thật với các gói giả lập riêng trong thư mục tạm: gói đúng/đường dẫn Unicode, không ghi đè báo cáo, file thay đổi hoặc bị mất, bỏ sót file trong manifest, manifest trống, vượt folder, đường dẫn tuyệt đối, đường dẫn trùng, thiếu hash và kích thước sai. Một bài có nhiều điều kiện kiểm tra.
- `npm run verify:local` chạy các bộ kiểm tra độc lập, ghi log và JSON tổng hợp; trả mã lỗi khi có bất kỳ bước thất bại. Các lượt lỗi vẫn giữ log riêng để đối chiếu.

## Kết quả xác nhận

Lượt cuối: `.bench-asr/verify-1793172868742/report.json`.

| Kiểm tra | Kết quả |
| --- | --- |
| Frontend | 81 đạt |
| Thao tác hàng đợi | 3 đạt |
| Script kiểm tra gói | 9 đạt |
| TypeScript/Vite | Thành công |
| Rust offline | 247 đạt, 3 bỏ qua có sẵn |

Hai bài Rust bổ sung dùng filesystem/FFmpeg thật: nguồn có metadata xoay 90° trong đường dẫn tiếng Việt được ghi phụ đề, xuất đúng kích thước hiển thị 240×320, lưu sang đường dẫn tiếng Việt và giải mã cả hình/tiếng không lỗi; đích có thuộc tính chỉ đọc gây lỗi ghi đè, giữ nguyên nguồn/bản cũ và không còn file tạm. Đây là kiểm tra file chỉ đọc, chưa thay thế kiểm tra thư mục bị từ chối quyền bằng ACL.

Fixture xoay ban đầu dùng metadata `rotate=90`, nhưng FFmpeg trên máy không ghi metadata đó khi remux; đã đổi sang `-display_rotation 90` và xác nhận rotation ở nguồn trước khi thử xuất. Không tính lượt fixture không tạo được rotation là lỗi sản phẩm. Bài này cần FFmpeg có tùy chọn `-display_rotation`.

## Giới hạn còn lại

Chưa xác nhận tải link thật YouTube/Douyin/TikTok trong lượt này, các API dịch, ma trận nguồn Anh/Trung/Nhật/Hàn, chất lượng nghe bằng người, đường GPU so với Whisper CLI và native Piper so với Web Worker. Chưa có máy/VM Windows sạch hay lượt cài/gỡ NSIS/MSI. Các phần này tiếp tục giữ trạng thái chưa nghiệm thu, không đánh dấu hoàn tất mục 7.

Build `npm run tauri build -- --no-bundle -- --offline` thành công. Gói mới nằm ở `output/windows-20261028-verification/` và `output/windows-20261028-verification.zip`, giữ nguyên các gói cũ. Script kiểm tra từ chính folder bàn giao báo `manifestPassed: true`, đủ 12 file; báo cáo tại `.bench-asr/verify-1793172868742/package-preflight.json`. Đã đọc lại 12 file bên trong ZIP, đối chiếu kích thước và SHA-256 với manifest. SHA-256 EXE: `332A19C749730F34E001813FC692734DE7CF8B76EC07BB98401B28CA408086E1`.

Lượt này xác nhận build và tính toàn vẹn gói, chưa mở EXE mới để nghiệm thu giao diện. Đây là gói chạy thử EXE kèm tài nguyên, chưa phải installer. Không commit.
