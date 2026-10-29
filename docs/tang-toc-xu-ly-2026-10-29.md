# Giảm thời gian xử lý — 29/10/2026

## Phụ đề thay Whisper

`pipeline/download.rs` yêu cầu yt-dlp ghi `.info.json` trong lần tải video hiện có. Không chạy thêm một lượt yt-dlp để dò phụ đề. `pipeline/source_captions.rs` chọn track trong `subtitles` có ngôn ngữ khớp `language` của video, hoặc mã ngôn ngữ gốc không có biến thể vùng. Không dùng `automatic_captions`, không chọn một ngôn ngữ khác để dịch qua tiếng trung gian và không đoán khi metadata thiếu ngôn ngữ.

Chỉ tải một track HTTPS, tối đa 8 MiB, timeout request 12 giây. Ưu tiên JSON3, SRT rồi VTT. Phụ đề rỗng, mốc không hợp lệ/chồng nhau, câu quá dài, thẻ markup chưa xử lý hoặc mốc vượt độ dài video đều quay về Whisper. Đây là kiểm tra cấu trúc, không chứng minh được bản phụ đề chính xác hay đầy đủ nội dung.

Track hợp lệ lưu theo định dạng Cue hiện có vào `transcript.json`; nguồn ghi ở `transcript-source.json`. Luồng tách tiếng/Whisper nhận ra cache, Xem trước và xuất dùng lại cùng file. Không thay lời chép/lời đã biên tập đã có, kể cả file cũ không đọc được. Chép lời lại xóa metadata nguồn phụ đề cùng transcript. Không gửi audio/video lên mạng.

Phạm vi: video tải bằng yt-dlp có metadata và track phù hợp. File local, Douyin và bản tải cũ thiếu metadata vẫn dùng cách hiện có. Đường Nghe trước lấy phụ đề không thay đổi trong lần này. Khi URL phụ đề trong metadata cũ hết hạn, app quay về Whisper.

## Dịch và đọc giọng cuốn chiếu

Sau khi hoàn tất duyệt tên và thuật ngữ, bản dịch được ghi xuống đĩa trước rồi mới chuyển các câu hợp lệ sang một tác vụ TTS. Chỉ dùng một tác vụ đọc giọng, hàng đợi tối đa hai lô; không tăng số request dịch. Giới hạn DeepSeek local vẫn là một request một lúc.

Chỉ áp dụng khi engine là giọng chạy trong cửa sổ app. Worker giữ giọng đã nạp giữa các lô. Piper native tiếp tục đọc cả lô trong một tiến trình, tránh tăng số lần nạp model. Chỉ chữ, Không dịch, việc đã biên tập và kết quả đã có giữ đường xử lý tương ứng hiện tại.

Câu dịch trống chưa được đọc thành im lặng khi các lượt thử lại còn đang chạy. Câu đã có WAV hợp lệ giữ nguyên; nếu bản dịch của một câu thay đổi, bỏ WAV cũ trước khi đưa sang TTS. Bước speak cuối chờ cả hai tác vụ xong, bổ sung phần còn thiếu và thực hiện rút gọn câu dài như trước. Việc căn mốc và ghép không chạy đồng thời với hai tác vụ này.

Hủy hoặc lỗi làm dừng cả hai tác vụ; scope chờ worker kết thúc trước khi trả về. Lỗi gốc được giữ khi tác vụ còn lại trả lỗi hủy. Không đổi định dạng translation/WAV hoặc cách duyệt tên. `timing.log` ghi `translate+speak` cho khoảng thời gian xử lý chồng nhau và `speak` cho phần kiểm tra/rút gọn sau đó, tránh cộng hai lần cùng một khoảng thời gian.

## Kiểm chứng

- `npm run verify:local`: frontend, queue, playback, windows-package, build và Rust đều PASS. Báo cáo: `.bench-asr/verify-1793241492999/report.json`.
- Sau các test bổ sung cuối: `cargo test --manifest-path src-tauri/Cargo.toml --offline --lib`: **284 passed, 0 failed, 3 ignored**.
- Test mới xác nhận speech bắt đầu trước khi producer dịch kết thúc; giữ thứ tự lô và ID câu; lỗi speech không kẹt hàng chờ đầy; lỗi dịch dừng/chờ speaker; bản dịch được lưu và WAV im lặng cũ bị bỏ trước callback TTS.
- Test phụ đề xác nhận chỉ chọn ngôn ngữ gốc do kênh cung cấp; từ chối auto/không rõ ngôn ngữ và mốc sai; cache đọc được bằng đường transcript hiện có; không thay dữ liệu cũ và không ghi transcript khi đã hủy.

Đã bổ sung [thử tốc độ trên video Douyin 6 phút](thu-toc-do-douyin-2026-10-29.md), có số đo và giới hạn do câu trả lời model thay đổi. Chưa kiểm thử tải track phụ đề thật từ nền tảng; nguồn Douyin không đi qua nhánh đó. Mức cải thiện phụ thuộc phụ đề có dùng được không, engine giọng và bước nào đang chiếm thời gian chính.
