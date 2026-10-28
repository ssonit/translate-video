# Rà soát trước khi push ngày 28/10/2026

Người dùng yêu cầu rà soát, commit và push các thay đổi nâng cấp. Phạm vi gồm workflow/xuất video/hàng đợi đã triển khai và bốn mốc phụ đề, chỉnh mốc, tách/gộp, lịch sử trong [kế hoạch học OpenCreator](ke-hoach-hoc-opencreator.md). Giữ Tauri, tên sản phẩm và identifier hiện tại.

## Lỗi tìm thấy và sửa

Chọn bản lịch sử gọi IPC bất đồng bộ. Trước sửa, nếu chọn A rồi B mà phản hồi A đến sau, A có thể ghi đè lựa chọn B. Một phản hồi cũ cũng có thể đổi trình phát sau khi chuyển sang việc khác hoặc bắt đầu xử lý.

`useWorkState` giữ kết quả/trạng thái bận mới nhất và số thứ tự yêu cầu. `selectVersion` chỉ áp dụng phản hồi của lần chọn mới nhất, đúng việc và còn rảnh; lỗi từ yêu cầu hết hiệu lực cũng không hiện lên việc mới. Chuyển việc làm hết hiệu lực các yêu cầu cũ, kể cả chuyển đi rồi quay lại cùng đường dẫn.

Thêm ba bài trong `scripts/test-playback-actions.mjs`, chạy action thật qua Vite và thay riêng IPC bằng Promise có thứ tự trả lời được điều khiển: chọn liên tiếp trả lời ngược; chuyển việc/bắt đầu chạy trước khi phản hồi; bỏ qua lỗi cũ nhưng giữ lỗi hiện tại. `verify:local` chạy thêm nhóm này.

## Kiểm tra mã nguồn

Lượt cuối của `npm run verify:local`: `.bench-asr/verify-1793198640126/report.json`.

| Nhóm | Kết quả |
| --- | --- |
| Frontend | 88 đạt |
| Thao tác hàng đợi | 3 đạt |
| Chọn phiên bản bất đồng bộ | 3 đạt |
| Kiểm tra gói Windows | 9 đạt |
| TypeScript/Vite | Thành công |
| Rust offline | 275 đạt, 3 bài ignore có sẵn |

`git diff --cached --check` không có lỗi khoảng trắng. Rà các file chuẩn bị commit không phát hiện mẫu token thật/private key hoặc harness tạm. Không đưa `output/`, `.bench-asr/`, EXE, model, giọng ONNX, key hay phiên đăng nhập vào Git. Đây là kiểm tra các mẫu nhạy cảm thường gặp, không phải chứng nhận an ninh độc lập.

## Giới hạn nghiệm thu

Đã đọc lại bằng chứng các lượt video thật, kiểm tra cache, hủy/chạy tiếp và lịch sử trước đó; không coi việc đọc báo cáo cũ là đã chạy lại dịch vụ hoặc nghe đánh giá chất lượng trong lượt này. Lượt lịch sử dùng clip 13,6 giây và có kiểm tra chọn/lưu chính xác bản xuất; xem [báo cáo mốc D](lich-su-ban-xuat-2026-10-28.md).

Chưa có máy/VM Windows sạch; chưa xác nhận cài/gỡ NSIS/MSI, video dài với nhiều tiếng nền bằng người nghe, mọi ngôn ngữ/dịch vụ và nguồn link. Không tự tải công cụ/model, chạy installer hoặc thêm kiểm tra AI. Bản EXE kèm tài nguyên là bản chạy thử trên máy phát triển, không phải installer đã nghiệm thu.

## Gói chạy thử cuối

`npm run tauri build -- --no-bundle -- --offline` thành công với mã cuối cùng, gồm bản sửa chọn phiên bản bất đồng bộ. Gói mới nằm ở `output/windows-20261028-prepush/`, ZIP cùng tên trong `output/`; giữ các gói cũ. Không đưa gói nhị phân này lên Git hoặc tạo GitHub Release.

Kiểm tra từ chính folder bàn giao báo `manifestPassed: true`, đủ 12 file. Đã đọc từng file bên trong ZIP và đối chiếu cả kích thước lẫn SHA-256 với manifest.

- SHA-256 EXE: `8101303DBA0ECA51009B73DB81B659FA9FC6E66EAA4D05111A53C90D1048DE2F`.
- SHA-256 ZIP: `A01127108D162AD29FA6C0E4A953D577409D6C927BC79F71641149D622E981C6`.
- Bằng chứng: `.bench-asr/prepush-20261028/package-preflight.json` và `zip-verification.json`.

Đã mở EXE cuối từ chính gói, kiểm tra giao diện và điều hướng màn Lồng tiếng → Đồ nghề → Lồng tiếng → Cài đặt → Lồng tiếng. Chip báo thiếu ba mục bắt buộc (model, giọng, Whisper) đúng với tài nguyên release chưa được chuẩn bị; Piper thiếu là tùy chọn. Nút Chạy khóa đúng. Không bấm tải/cài/bật server hoặc đổi cấu hình/key của người dùng.

IPC lịch sử trong EXE cuối đọc được metadata, chọn đúng file và trả đúng cấu hình cũ. Video lịch sử được WebView giải mã thực sự: 270×480, dài 13,633 giây. Không có lỗi JavaScript được ghi nhận trong lượt thử. Bằng chứng và ảnh: `.bench-asr/prepush-20261028/release-smoke.json`, `release-main.png`, `release-settings.png`. Các cửa sổ thử được đóng sau nghiệm thu.
