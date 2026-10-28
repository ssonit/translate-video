# Mốc D — Lịch sử bản xuất

Hoàn tất và kiểm tra local ngày 28/10/2026. Giữ Tauri, tên app, các dịch vụ và giọng có sẵn. Không commit.

## Hành vi

- **Lịch sử & cấu hình** trong màn kết quả liệt kê bản cũ theo thời điểm, phân biệt video/audio và ghi thay đổi: chữ, mốc, tách/gộp, kiểu chữ, khung hình, giọng, âm lượng, logo/vùng che hoặc lựa chọn dịch.
- **Cấu hình** chỉ đọc thông tin. **Xem** chuyển trình phát tới đúng file; có thể chọn bản có chữ, không chữ trong cùng lần xuất. Nút tải video/audio dùng đúng bản đang chọn.
- **Nạp cấu hình để sửa tiếp** lấy lựa chọn hình/tiếng về màn soạn. Lời và mốc vẫn là dữ liệu biên tập hiện tại; dịch lại dùng dịch vụ đang chọn. Không phục hồi API key hay tự đổi nhà cung cấp.
- Đang lồng tiếng, ghép chữ hoặc nghe liền mạch thì khóa chuyển bản/nạp cấu hình. IPC kiểm tra trạng thái ở cả trước và sau khi đọc; vẫn xem thông tin lịch sử được.
- Bản cũ không có metadata khớp file hiện **Bản cũ · không rõ thay đổi**; mở/lưu được, không nạp cấu hình suy đoán. File đã bị thay đổi không được nhận cấu hình cũ chỉ vì trùng tên.

## Dữ liệu và phục hồi

Mỗi lần xuất thành công có folder `cu/ban-v-…/`, gồm kết quả và `version.json` phiên bản 1. Bản có chữ có thể giữ cả MP4 không chữ và SRT đi kèm. `history-current.json` trỏ bản vừa chốt. Lần chạy lại dùng đúng cùng nội dung, cấu hình và kết quả không tạo thêm bản trùng.

Metadata dùng danh sách trường được phép: ngôn ngữ, giọng, cờ dịch/xuất, cấu hình render, mã dịch vụ/model, thời điểm, lý do và hash nội dung/mốc/cấu trúc. Không ghi đối tượng cấu hình tài khoản hoặc API key. SHA-256 nhận diện file kết quả; đây không phải chữ ký xác thực nguồn metadata.

Giữ media bằng hard link trên filesystem hỗ trợ, nếu không thì chép theo luồng có kiểm tra hủy. Không nhân bản `lines/*.wav`, transcript hoặc toàn bộ dữ liệu làm việc cho mỗi bản. Khi app thay kết quả bằng file mới, liên kết cũ vẫn giữ đúng media đã xuất. Các bản media khác nhau vẫn cần dung lượng; người dùng xóa từng bản qua thao tác có xác nhận hiện có, chưa tự dọn theo hạn mức.

Ghi vào folder `.pending-v-…`, đối chiếu hash rồi đổi tên. Lỗi/hủy trước chốt dọn folder dở. Trong sửa câu, ID lịch sử nằm trong nhật ký `_undo`: bản mới bị ẩn cho tới cờ `done`; phục hồi xóa bản chưa chốt và trả lại pointer/bản kết quả trước đó. Các folder `cu/` cũ và `incomplete/` được giữ. Bản trùng byte trong kho cũ được ẩn khỏi danh sách khi đã có snapshot tương ứng, không xóa dữ liệu cũ.

Các lệnh mới: `version_details`, `select_version`, `version_config`. Chỉ chấp nhận media của đúng folder việc hoặc một folder con của `cu/`; từ chối đường dẫn ngoài việc, file partial và bản chưa chốt.

## Kiểm chứng

`npm run verify:local` đều PASS: 88 kiểm thử frontend, kiểm tra hàng đợi, kiểm tra gói Windows, build và 275 kiểm thử Rust; 3 bài Rust được ignore từ trước. Báo cáo: `.bench-asr/verify-1793186226951/report.json`.

Sáu bài Rust mới dùng FFmpeg tạo media nhỏ, kiểm tra giữ file/cấu hình qua nhiều lần xuất, thay lời, không lộ key và không sao chép WAV; lỗi/hủy không đổi bản thành công; bản trong giao dịch bị ẩn tới `done`; metadata cũ không bị suy đoán; từ chối đường dẫn khác việc; bản media lưu thiếu được tạo lại; audio `.nghe.m4a` còn trong lịch sử sau lần xuất video tiếp theo và lưu đúng byte. Bài phục hồi retake hiện có được bổ sung kiểm tra xóa snapshot chưa chốt và phục hồi pointer cũ.

Kiểm tra qua IPC thật và WebView của Tauri trên bản sao clip 13,6 giây, giọng đã chọn `ngochuyennew`:

1. Xuất hai kiểu chữ `#80e0ff` và `#ffcc00`; bản đầu và cấu hình cũ còn đúng.
2. Sửa một câu và xuất thành công: một WAV thay đổi, thêm một bản lịch sử; chuyển bản/nạp cấu hình trong khi đang đọc bị từ chối.
3. Hủy tại bước fit: hash kết quả và pointer cũ giữ nguyên, không thêm bản, dọn `_undo`.
4. Chỉ xem cấu hình không áp dụng; bấm nạp trả đúng kiểu chữ cũ và không sửa `render.json` của việc hiện tại.
5. Lưu riêng video có chữ và không chữ: hash khớp chính xác từng file; tên thiếu đuôi được nối `.mp4`.
6. Xuất audio, tiếp tục xuất video; audio cũ vẫn chọn được, tải khớp hash. Mở lại trang đọc đúng metadata trên đĩa.
7. Bản thiếu metadata khóa nút nạp; các nút chuyển bản bị khóa khi giao diện bận. Kho lịch sử không có WAV.
8. Video lưu cũ thực sự được WebView giải mã; kiểm tra kích thước/độ dài và bố cục popup ở cửa sổ rộng/hẹp 430 px.

Báo cáo và ảnh: `.bench-asr/history-20261028/report.json`, `history-wide.png`, `history-narrow.png`. Harness chỉ dùng để thử local; đã khôi phục nguyên byte `src/main.tsx` trước build, không có harness trong `dist`.

## Giới hạn

Chưa có phục hồi toàn bộ bản thảo theo phiên bản, so sánh hai video cạnh nhau hoặc giới hạn dung lượng tự động. Nạp cấu hình không có nghĩa tái tạo chính xác bản cũ; chính file media lưu trong lịch sử là bản có thể mở và tải nguyên vẹn. Lịch sử không bảo vệ khỏi người dùng sửa/xóa trực tiếp các file trên đĩa.

Lượt này xác minh lịch sử, lưu file và giao dịch dữ liệu; không chấm lại chất lượng nghe, bản dịch, dịch vụ trực tuyến, tải link, bộ cài hoặc máy Windows sạch. Hạng mục kiểm tra dịch có mục tiêu trong kế hoạch vẫn là phần tùy chọn chưa triển khai.
