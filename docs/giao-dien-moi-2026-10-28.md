# Giao diện desktop mới — 28/10/2026

## Thay đổi

- Navigation riêng: Lồng tiếng, Hàng đợi, Thư viện, Bộ cài đặt, Cài đặt. App vẫn mở vào Lồng tiếng và kiểm tra máy ở nền.
- Thư viện thành trang rộng, có lọc Tất cả / Đang dở / Đã xong, tìm theo tên/ngôn ngữ/nguồn, thời gian cập nhật và hình xem trước local. Chỉ cấp quyền đọc hình khi thẻ đi vào vùng hiển thị; nguồn lỗi giữ placeholder.
- Hàng đợi đặt cùng tiến độ xử lý, duyệt tên, chính sách lỗi, chọn nhiều việc, đổi thứ tự, cập nhật cấu hình và lịch sử. Không có việc thì hiện trạng thái sẵn sàng thay vì báo Lỗi. Chạy tiếp hàng đợi tuân theo khóa đồ nghề.
- Bộ cài đặt có trang quản lý, lưu lựa chọn hiện tại, áp dụng, đổi tên và xóa. Màn tạo việc vẫn có lựa chọn nhanh. Không đổi schema hoặc key lưu các bộ hiện có.
- Hình ảnh/chữ/âm thanh tách khỏi form tạo việc thành trang cấu hình riêng, chia ba tab: Hình & chữ, Âm thanh, Đoạn thử 8 giây. Sửa bản xuất nạp cấu hình đã lưu rồi xuất lại qua pipeline hiện có.
- Màn kết quả có video và lời thoại cạnh nhau; kéo vạch chia hoặc dùng mũi tên trên bàn phím để đổi độ rộng. Cửa sổ hẹp xếp thành các vùng theo chiều dọc.
- Timeline nằm dưới toàn chiều ngang vùng làm việc; chọn câu đồng bộ với danh sách lời thoại và tự đưa khoảng thời gian vào vùng nhìn thấy. Tách/gộp ở nhóm mở rộng. Mốc, nghe lặp và hoàn tác giữ logic hiện có.
- Thanh xuất cố định có trạng thái nháp, bỏ sửa và xuất lại. Giữ nút tải bản video đã xong khi có việc khác đang chạy.
- Work và TTS bridge tiếp tục sống qua điều hướng. Màn tạo việc và kết quả giữ mounted để không mất nháp. Việc tải lời thoại phụ thuộc video, không xóa dữ liệu chỉ vì đổi màn. Rời preview tạm dừng phát, giữ vị trí/phần đang nghe và có đường quay lại.
- Nháp lời đang nhập và mốc chưa xuất được giữ trong phiên khi chuyển màn. Mở video khác hoặc bắt đầu việc thay thế có hộp thoại xác nhận bỏ nháp. Không tuyên bố nháp đã lưu qua lần đóng app.
- Giữ Tauri, Rust commands và dữ liệu pipeline; không đổi tên sản phẩm hoặc tải công cụ/model/voice.

## Màu và control

Học cách phân vai màu của `D:/Project/folia-major`, không chép hệ sinh theme hoặc toàn bộ giao diện của repo đó:

- Nền than `#09090B`, surface `#121215`, control/menu `#1C1C21`.
- Chữ `#F4F4F5`, chữ phụ `#A1A1AA`, nhấn cam `#F97316` và chữ tối trên nút cam.
- Giữ font Be Vietnam Pro, tăng cỡ nền lên 15 px; nút thường tối thiểu 40 px. Focus rõ và icon điều hướng có accessible name/tooltip.
- Bỏ các grid kết quả cũ phụ thuộc video dọc/ngang để không xung đột với grid mới. Màu phụ đề/render vẫn do người dùng chọn, độc lập với theme UI.

Đo bằng công thức độ chói sRGB trên các cặp màu gốc:

| Cặp | Tương phản |
| --- | --- |
| Chữ chính / surface | 17.01:1 |
| Chữ phụ / raised | 6.62:1 |
| Chữ nút / cam | 6.63:1 |
| Cam / surface | 6.67:1 |
| Viền control / raised | 3.51:1 |

Kết quả chi tiết local: `.bench-asr/studio-contrast.json`. Các số này không thay cho kiểm tra mọi trạng thái alpha, disabled và nền video.

## Kiểm chứng

`npm run verify:local` đã qua cả sáu nhóm:

- Frontend: 89 test qua, gồm lọc thư viện theo nguồn và trạng thái.
- Hành động hàng đợi: qua.
- Hành động phát/chọn kết quả: qua.
- Quy tắc đóng gói Windows: qua.
- TypeScript + Vite build: qua.
- Rust: 275 test qua, 3 test bỏ qua theo cấu hình hiện có.

Log local: `.bench-asr/verify-1793203680129/report.json`. Sau các điều chỉnh bố cục/nhãn và chọn nhanh bộ, chạy lại build frontend và kiểm tra diff.

Đã khởi động bằng `npm run tauri dev`, đọc được màn chính và kết quả kiểm tra máy qua accessibility của cửa sổ thật. Công cụ screenshot Windows trả nhầm hình cửa sổ khác và không kích hoạt đúng cửa sổ, nên không dùng ảnh đó để nghiệm thu.

Kiểm tra UI trong trình duyệt riêng bằng fixture local `.bench-asr/studio-ui.html`, mock IPC và video màu do FFmpeg tạo, không chạy dịch vụ AI/TTS hoặc thay dữ liệu việc thật:

- Mở các màn và các tab cấu hình.
- Sửa mốc → sang Bộ cài đặt → quay lại: còn nháp, Hoàn tác mốc vẫn dùng được.
- Đang gõ lời → sang Hàng đợi → quay lại: còn nội dung chưa bấm Xong.
- Chọn câu cuối từ lời thoại: timeline chọn câu 12 và chuyển vùng từ 0–10 sang 10–20 giây.
- Vạch chia bằng ArrowRight đổi từ 52% sang 54%.
- Kiểm tra 1080 × 760, 1366 × 768 và 760 × 760; ở 760/1366, `scrollWidth` bằng chiều rộng viewport, không tràn ngang toàn trang.
- Ảnh local: `.bench-asr/studio-main-1366.png`, `.bench-asr/studio-editor-1366.png` (dữ liệu minh họa kiểm thử).

## Giới hạn còn cần nghiệm thu thực tế

- Chưa chạy lại trọn job với provider thật và đọc giọng thật trong lúc liên tục chuyển các màn sau thay đổi UI này.
- Chưa xác nhận trực tiếp Windows display scale 125%/150%, mọi kích thước màn hình hoặc installer trên máy sạch.
- Theme sáng và nhiều theme vẫn để sau. Dịch vụ/key và kiểm tra giọng vẫn dùng các control hiện có tại Đồ nghề, mở từ Cài đặt; lựa chọn dịch vụ nhanh vẫn có trên màn Lồng tiếng.
- Độ rộng panel giữ trong phiên, chưa lưu riêng qua lần đóng app. Timeline có thu gọn và chiều cao giới hạn; chưa có thanh kéo thay đổi chiều cao timeline.
- Các nháp lời/mốc ở UI chưa tự lưu ra đĩa khi đóng ứng dụng. File nền, cache và quy tắc xuất không thay đổi.

Không commit trong đợt này.
