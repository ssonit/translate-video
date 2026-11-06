# Sửa tiếp checklist — 06/11/2026

Tiếp trên cây thay đổi chưa commit của đợt trước. Không đổi tên sản phẩm, key hay dịch vụ dịch được người dùng chọn; không cài công cụ và không đụng `omniget/`.

## Kết quả sửa

**Công bố file sau mã hóa:** ghép video, đốt chữ và xuất audio dùng chung bước công bố. Kiểm tra yêu cầu hủy ngay trước khi đổi tên file tạm; lỗi/hủy dọn file tạm. Đốt chữ và xuất audio không còn xóa bản đích trước khi đổi tên, nên lỗi đổi tên giữ nguyên bản cũ. Kiểm bằng file đích bị Windows khóa và bằng hủy sau khi file tạm đã hoàn tất.

**Lịch sử bản xuất:** một snapshot có thể đang che các bản sao legacy giống hệt nó. Xóa snapshot trước đây làm bản sao đó hiện lại, khiến số bản trông như không giảm. App ghi nhận đúng đường dẫn/hash của các bản sao đã bị ẩn vào `history-hidden-aliases.json`, chỉ xóa folder người dùng chọn. Bản sao và bản đang dùng giữ nguyên byte; nếu người dùng thay bản sao bằng nội dung khác thì nó hiện lại. File mới là metadata bổ sung; folder việc cũ chưa có file này vẫn đọc bình thường.

**Lỗi dịch qua HTTP:** lỗi 401/403 nói rõ key/quyền truy cập và không hiện body phản hồi có thể chứa key. Các lỗi HTTP khác che key trong body trước khi rút gọn. Thêm kiểm thử JSON hỏng, timeout và key phản chiếu; bài 429/hủy được đặt tên để nằm trong nhóm `translate` của checklist.

## Bằng chứng

- Rust: **465 đạt, 0 trượt, 7 bỏ qua**; frontend: **30 đạt**; build frontend và Rust đạt.
- Nhóm `translate`: **21 đạt, 0 trượt, 1 bỏ qua**, có bài JSON hỏng, thiếu câu, 429/hủy, timeout và che key.
- Trong **Tauri thật**, xóa một snapshot làm danh sách UI và `list_kept` cùng giảm **1 → 0**. Chỉ folder được chọn mất; hash của bản đang dùng và bản sao legacy không đổi. Câu trả lời hộp thoại được nạp bằng công cụ test trên fixture QA, không dùng để nghiệm thu hộp thoại Windows.
- Bằng chứng: `.bench-asr/fixes-20261106-followup/`, gồm `native-results.json`, `history-after.png`, `rust-tests.log`, `translate-list.log`, `translate-tests.log`, `frontend-tests.log`, `frontend-build.log` và `native-restored.json`.

## Gemini thật

Key đã lưu lấy được danh sách model (HTTP 200), gồm `gemini-3.8-flash`. Lượt pipeline thật chép được bốn câu, nhưng yêu cầu tìm tên hết giờ sau 180 giây. Một yêu cầu tạo nội dung tổng hợp độc lập ngoài app cũng timeout sau 20,171 giây. Đã yêu cầu dừng lượt pipeline; không dùng metadata model làm bằng chứng dịch thành công và không kết luận lỗi nằm ở app hay dịch vụ/mạng.

Chỉ lời thoại của clip thử công khai và câu thử tổng hợp được gửi tới Gemini; không gửi audio/video, không in key. Các ca ChatGPT, Claude và DeepSeek API vẫn chờ key theo lựa chọn của người dùng.

## Trạng thái checklist

Ca tự động **4.21** đủ bằng chứng mới để ghi đạt. Các sửa hủy/công bố file và lịch sử đã có hồi quy mới theo phạm vi trên; các ca nhiều thao tác vẫn cần chạy đủ mọi biến thể/hồi quy được ghi trong checklist. Không coi kết quả unit test là nghiệm thu toàn bộ hệ thống.

Các quan sát cũ về thuật ngữ cần đối chiếu trạng thái hiện tại: ca **9.9 đã đạt** trong checklist và có bằng chứng mới hơn lỗi cũ; không tiếp tục coi lỗi log cũ là lỗi đang mở. Các mục lệch tiêu chí và các ca thiếu điều kiện thử vẫn giữ riêng, không thay hành vi sản phẩm chỉ để tăng số ca đạt.

## Phạm vi theo yêu cầu mới của người dùng

Chỉ sửa các lỗi sản phẩm đã có bằng chứng. Để sau các ca thiếu API key, thông tin đầu vào, môi trường thử hoặc quyết định tiêu chí; không yêu cầu bổ sung các điều kiện đó trong đợt sửa này. Gemini đã timeout cả trong app lẫn phép thử độc lập nên chưa đủ bằng chứng quy lỗi cho mã; dừng kiểm dịch vụ này, không tiếp tục thử API. Lượt test đã được hủy xong, app/Vite đã đóng và cả ba file cấu hình khôi phục đúng byte (`native-restored.json`).
