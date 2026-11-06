# Sửa lỗi checklist — 06/11/2026

Nền mã `2419504`, thay đổi chưa commit. Đây là báo cáo sửa và hồi quy có phạm vi; không phải chứng nhận đã đóng toàn bộ 38 ca trượt hoặc đủ điều kiện phát hành.

## Đã sửa

| Nhóm | Thay đổi |
| --- | --- |
| Trạng thái việc cũ sau hủy/chạy mới | Đánh dấu lượt chạy trong cửa sổ; bỏ phản hồi khôi phục/poll đến muộn và sự kiện hoàn tất thuộc thời điểm trước lượt chạy mới. |
| Chạy tiếp từ Thư viện | Lỗi/“Đã dừng” không còn làm việc dở bị nhận là đang chạy rồi chuyển nhầm sang Hàng đợi. |
| Xem trước / Nghe trước | Nối lại hai nút vào đường `runLive` hiện có, giữ kiểm tra nguồn và khóa khi thiếu công cụ. Không thay đường Mở nguồn & sửa nháp. |
| Nháp sau khi sửa kết quả | Khi quay lại Lồng tiếng, đọc lại lời/cấu trúc mới. Nếu còn thay đổi chưa lưu thì giữ nháp và báo xung đột. |
| Hai nơi chọn dịch vụ | Đồ nghề đọc lại lựa chọn dịch khi đổi màn, tránh giữ lựa chọn cũ từ lần mount đầu. |
| Dán danh sách tên | Báo số dòng trùng bị bỏ qua, kể cả trùng với hàng đã nhập; số tên thêm phản ánh đúng các cặp được chấp nhận. |
| Sửa tên | Giữ bản xuất cũ trong lịch sử rồi xóa bản dịch, thuật ngữ và WAV phụ thuộc; giữ lời Whisper. Danh sách tên không đổi không xóa cache. Cập nhật lời giải thích trên form. |
| Audio và SRT | Dùng lại audio không ghi đè SRT đã có bằng mốc video. Xuất mới phải lấy được mốc từ tiếng đọc, không âm thầm dùng mốc video khi lỗi. Câu có chữ nhưng tiếng bị im lặng hoàn toàn báo lỗi thay vì xuất khối SRT có thời lượng bằng 0. |
| Mức tiếng gốc | Nhân đôi tín hiệu điều khiển mono sang stereo ở hệ số 1, loại phần suy hao 3 dB ngoài khoảng giọng. |
| Logo hỏng | Giải mã thử ảnh bằng FFmpeg trước khi chấp nhận/cất logo; không chỉ kiểm magic header. |
| Audio → tải hình | Không nhận lại webm chỉ có tiếng như một video; tải hình vào nhánh riêng và giữ nguyên đường dẫn nguồn audio cũ. |
| Nguồn local | `video_brief` đọc độ dài/kích thước bằng ffprobe; từ chối file không phải video trước khi đổi nguồn/xếp hàng. Màn chọn nguồn hiện thông tin đó. Pipeline từ chối nguồn hỏng trước khi tạo folder việc. |
| Thông báo lỗi | Lỗi mạng/proxy/DNS nói rõ cách kiểm tra mạng, không khuyên cập nhật yt-dlp cho lỗi kết nối. Video không có âm thanh có thông báo riêng. |
| Cửa sổ / hướng dẫn | Đặt kích thước tối thiểu 760×600; sửa nhãn “Bản dịch” trong hướng dẫn thành “Phụ đề (không đọc)”. |

## Đã kiểm tra

- `npm run build`: đạt; còn cảnh báo bundle trên 500 kB.
- Rust: **460 đạt, 0 trượt, 7 bỏ qua**. Các bài mới kiểm ảnh hỏng/ảnh tốt, cache chỉ audio so với video thật, lỗi mạng/DNS, câu im lặng và xóa cache sau sửa tên. Bài FFmpeg kiểm mức tiếng ở khoảng nghỉ theo trị tuyệt đối, ngoài kiểm tỉ lệ hạ tiếng.
- Frontend: **30 bài đạt**, gồm dán tên, hàng đợi, demand và định dạng trạng thái.
- Kiểm UI với backend giả: phản hồi khôi phục cũ và sự kiện hoàn tất cũ không kéo việc A trở lại sau B; quay về nháp cập nhật lời mới; nháp chưa lưu không bị ghi đè. Các bài này không thay nghiệm thu backend thật.
- App Tauri thật: command từ chối PNG 12 byte và file không phải video; ffprobe nguồn 19,014 giây, 320×240; sửa tên xóa cache nhưng giữ hash lời chép; xuất audio thật hai lần giữ nguyên hash `.m4a`/`.srt`; nguồn không có tiếng báo lỗi rõ; thấy hai nút Xem trước/Nghe trước theo loại kết quả. Nguồn cho phần chọn file được nạp bằng stub, không dùng làm bằng chứng hộp thoại Windows.
- Đã hoàn trả ba file cấu hình đúng byte, đóng app/Vite test. Không đổi key, cài công cụ, đổi tên sản phẩm, tắt server DeepSeek của người dùng hoặc đụng tới `omniget/`.

Bằng chứng ở `.bench-asr/fixes-20261106/`: `rust-tests.log`, `frontend-build.log`, `frontend-tests.log`, `ui-results.json`, `native-results.json`, `native-progress.json`, `native-baseline.json`, `native-restored.json` và ảnh UI.

## Chưa được coi là đã nghiệm thu xong

Giữ các ca checklist ở trạng thái cũ cho tới khi chạy đủ từng thao tác và hồi quy đi kèm; không chuyển sang đạt chỉ vì sửa mã hoặc unit test qua. Đường vào preview đã được khôi phục, nên lý do “không có nút” của 7.2–7.15 đã được gỡ; cần chạy lại ma trận này. DeepSeek thật vẫn cần khôi phục khả năng trả lời trước khi nghiệm thu các ca dịch phụ thuộc nó.

Các lỗi/quan sát khác chưa đóng: lịch sử xóa bản cũ ở 11.13 (bằng chứng cũ còn nhiều archive legacy; xóa snapshot có thể làm một bản trùng trước đó bị ẩn hiện lại, chưa nên kết luận là UI không refresh), ma trận đổi nguồn hỏng/chạy lại, hủy ghép/đốt chữ, thuật ngữ và Gemini thật. Bản media cũ đã có SRT sai không tự bị ghi lại. Các mục lệch tiêu chí (audio liền mạch không cần dài bằng video, đếm bản cất so với cả bản hiện tại, giữ việc đang dở khi mở lại app, model local chỉ có hai lựa chọn) cần được phân biệt với lỗi sản phẩm trước khi đổi hành vi. Các ca thiếu key, môi trường offline và người nghe vẫn chưa đủ điều kiện nghiệm thu.
