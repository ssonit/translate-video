# Xử lý khi cần — đợt 1, ngày 29/10/2026

Đợt này triển khai nền lưu file và quyền sở hữu phản hồi TTS trong giai đoạn 1. Chưa hoàn tất giai đoạn 1 và chưa đổi thao tác mở nguồn/xem trước/xuất. Giữ nguyên định dạng file việc cũ và hợp đồng công bố/dọn dữ liệu của `watch.rs`.

## Đã tích hợp

- `atomic_file.rs`: tạo file tạm riêng cạnh đích bằng `create_new`, tên gồm tên file gốc, PID và số thứ tự. Ghi toàn bộ, `sync_all`, đóng handle rồi rename thay đích. Không xóa đích trước khi rename; lỗi thông thường dọn đúng file tạm của lần ghi đó. JSON và WAV cùng tên gốc không còn tranh `*.tmp`.
- `job::write_json` và `wav::write_wav` dùng cơ chế này, giữ schema JSON và PCM WAV hiện có. Cụm dịch đã chốt và từng câu TTS được hưởng đường ghi mới; chưa bổ sung giao dịch nhiều file.
- `tts-request`/`tts-clean` có mã phiên riêng. Cửa sổ gửi mã đó khi lưu PCM, hoàn tất đọc và trả chữ đã chuẩn hóa. Rust kiểm tra phiên dưới cùng mutex với lúc commit WAV, không cho response cũ tìm đường dẫn theo số câu của việc mới.
- Hủy đặt cờ trước, thu hồi phiên/các đường dẫn và đánh thức luồng đang chờ. Khi trả lỗi, lệnh hủy worker riêng có mã phiên nên không dừng nhầm yêu cầu kế tiếp. Cleanup cũ chỉ xóa phiên do chính nó sở hữu.
- Cửa sổ kiểm tra quyền sở hữu sau nạp model, trước đọc và trước gửi PCM; phần `finally` cũ không mở khóa yêu cầu mới. Giữ cơ chế thử lại câu ngắn hiện có.
- Chỉ tăng số câu đã ghi sau commit WAV thành công. Không nhận ghi lặp cùng câu, PCM rỗng, tần số không hợp lệ hoặc dữ liệu thiếu nửa mẫu; báo lỗi nếu bộ đọc báo xong khi chưa gửi đủ câu.

## Kiểm chứng

Các kiểm tra tự động bổ sung dùng file tạm thật và các promise được điều khiển thứ tự, không tải model hay gọi dịch vụ:

- Ghi JSON và WAV cùng tên gốc song song; giữ file `.tmp` và file dở từ trước.
- Rename thất bại vẫn giữ đích, dọn file tạm do lần ghi đó tạo.
- PCM/finish từ phiên cũ không ghi đè hoặc hoàn tất việc mới dùng cùng số câu.
- Ghi lặp không làm tăng tiến độ; PCM sai không thay checkpoint cũ; thiếu câu không báo thành công.
- Hủy thu hồi quyền ghi và đánh thức waiter; chuẩn hóa chữ trả muộn không hoàn tất phiên native mới.
- Hủy lúc nạp model không đọc/ghi tiếp; PCM cũ đến sau khi phiên mới bắt đầu không được gửi lưu; cleanup/hủy riêng của phiên cũ không mở khóa/dừng phiên mới.
- Lỗi lưu WAV dừng các câu sau và trả lỗi đúng mã phiên.

`npm run verify:local` đã qua cả sáu nhóm: frontend (95 test), hàng đợi, playback, kiểm tra gói Windows, build frontend và Rust offline (297 qua, 4 bỏ qua). Log và báo cáo: [report.json](../.bench-asr/verify-1793250282504/report.json). Đây là kiểm tra local, chưa xác nhận model/dịch vụ thật hoặc installer trên máy Windows sạch.

## Phần còn lại

Cập nhật sau đợt này: [đợt 2 đã thêm manifest/revision và dấu xác nhận WAV](trien-khai-khi-can-dot-2-2026-10-29.md). Phạm vi bên dưới là trạng thái tại cuối đợt 1.

Giai đoạn 1 còn manifest nháp có phiên bản/revision và dấu vân tay dữ liệu phụ thuộc, nhập việc cũ có phục hồi, vùng sẵn sàng bền vững và bộ điều phối chung cho chuẩn bị/nghe/xuất. Mã phiên TTS trong đợt này chỉ bảo vệ từng yêu cầu đọc, chưa thay revision nháp hoặc cache key theo nội dung/giọng.

Chưa nghiệm thu đóng tiến trình app ngay trong khi ghi, mất điện, khôi phục giữa nhiều file hoặc hủy toàn pipeline với model thật. Khi tiến trình bị kết thúc cưỡng bức, file tạm riêng có thể còn lại; nó không được đọc như checkpoint JSON/WAV và đợt này chưa tự quét xóa các file đó. `sync_all` trước rename không phải cam kết chống mọi lỗi ổ đĩa/mất điện.

Preview cũ vẫn chỉ công bố cache chính khi hoàn tất và vẫn dọn `watch/` khi dừng. Chưa có preview theo vùng, lưu sửa tách khỏi dựng, hay xuất từ nháp thiếu; các giai đoạn 2–5 giữ nguyên kế hoạch.
