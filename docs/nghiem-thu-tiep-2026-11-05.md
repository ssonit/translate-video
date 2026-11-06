# Nghiệm thu tiếp ngày 05/11/2026

Mã kiểm tra: `241950405acbbe5407ec7f66828fd87efa9b7561`. Lượt này chỉ kiểm thử, ghi bằng chứng và cập nhật checklist; không sửa mã sản phẩm, không commit. Các ca đạt từ lượt trước không mặc nhiên được coi là đã chạy lại trên HEAD này.

## Kết quả

Phần A có 240 ca: **170 đạt, 38 trượt, 32 chưa nghiệm thu, 0 chưa ghi kết quả**. Ca P0 đạt 49/68. Phần B còn nguyên 18 điều kiện chặn phát hành. Không còn ô kết quả trống không có nghĩa đã nghiệm thu xong; Phần A đạt cũng không thay thế Phần B.

- Đạt thêm **3.16**: app thật báo video YouTube không tồn tại; không còn media hoặc folder việc dở; nhập nguồn hợp lệ thì nút Chạy dùng lại được.
- Đạt thêm **14.4**: đã đóng các tiến trình app/Vite test; cổng 1420, 9226, 11434 đóng. DeepSeek của người dùng vẫn nghe ở 8000, PID con 8524.
- Đạt thêm **14.6**: đối chiếu đủ 240 ca, trạng thái, bằng chứng và mục B của các ca chưa nghiệm thu; không còn lỗi cấu trúc checklist.
- **6.22 chuyển trượt**: quét 673 file, 671 đạt kiểm tra cơ bản; hai SRT của kết quả Audio tiếng Nhật thật có khối thời lượng bằng 0. Hai audio lịch sử gốc còn có SRT dài hơn tiếng khoảng 10–11 giây; hai fixture thư viện là bản sao cùng lỗi. Ghi `BUG-ACCEPT-01` và `BUG-ACCEPT-02`, chưa sửa.
- **5.8**: 8 WAV giữ nguyên cả SHA-256 và mtime sau tua/phát. Clip B hết lời trước 45 giây và vùng đầu đã có đủ các câu; chưa chứng minh được chuẩn bị một vùng có lời mới chưa có cache.
- **13.3**: quét cuối 84.521 file / 5.202.697.898 byte sau khi đóng app và hoàn trả cấu hình: 0 hit, đối chứng đạt. Còn 3 DLL TEMP bị EPERM, nên chưa đủ phạm vi để nghiệm thu; không in key.
- `cargo build --offline` thành công. Không chạy lại mọi P0 vì người dùng yêu cầu sửa lỗi ở phiên sau.

## Các điều kiện đang chặn 32 ca

| Điều kiện | Ca |
| --- | --- |
| UI không có đường bấm Xem trước/Nghe trước theo checklist; cần xử lý OBS-7.1 hoặc chốt yêu cầu ở B17 | 7.2–7.15 (14 ca) |
| Chờ key thật; người dùng đã chọn hoãn | 4.18, 4.19, 4.20 |
| Cần môi trường đo DNS/mạng đầy đủ, quyền tạo rule riêng tiến trình hoặc VM cô lập | 3.17, 8.10, 13.2, 13.12 |
| Server DeepSeek hiện có không trả câu thử sau 45,063 giây; không khởi động lại server người dùng | 4.13 |
| Fixture Clip B không có lời ở 45 giây; cần kiểm vùng mới có lời ngoài vùng đã cache | 5.8 |
| Cần người nghe xác nhận hai người nói và giọng thành phẩm | 6.20 |
| Không có baseline cũ cùng điều kiện để kết luận ngưỡng hiệu năng | 6.21 |
| Nút Tắt DeepSeek bị khóa khi đang chạy; tiêu chí yêu cầu bấm lúc dịch cần chốt lại | 8.12 |
| Tiêu chí giữ đề xuất AI trái hành vi Giữ nguyên lời chép trong mã; cần quyết định B17 | 9.3 |
| Chưa có ứng dụng mặc định dùng được để nghiệm thu mở MP4/M4A/SRT ngoài app | 11.21 |
| Còn file TEMP bị hệ điều hành từ chối đọc, không đủ phạm vi quét key | 13.3 |
| Snapshot cấu hình người dùng lúc 22:20 của phiên trước đã mất; không đè backup 04/11 lên lựa chọn mới | 14.1 |
| Chờ chốt baseline trước khi xóa backup key cũ; giữ output và bằng chứng | 14.3 |
| Chờ phiên sửa lỗi và chạy lại P0 sau sửa cuối | 14.5 |

Đã hoàn trả cả ba file cấu hình đúng byte theo snapshot sau khi guard phiên cũ kết thúc; hash khớp `final-sweep-1105-e/restored.json`. Đây không phải chứng nhận khôi phục được lựa chọn 22:20 đã mất. Không đổi voice, Piper, Whisper, ACL hoặc tường lửa. Không đụng tới `omniget/`.

Chi tiết từng ca và sổ lỗi: [checklist](checklist-test-toan-he-thong-2026-11-04.md). Bằng chứng lượt này: `.bench-asr/test-20261104/evidence/acceptance-next-1105/`, gồm `summary.json`, `checklist-audit.json`, `media-classification.json`, `config-restored.json`, `deepseek-diagnostic.json` và các bản quét bí mật.
