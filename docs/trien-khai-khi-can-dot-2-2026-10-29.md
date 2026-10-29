# Xử lý khi cần — đợt 2, ngày 29/10/2026

Đợt này thêm manifest nháp và dấu xác nhận WAV trong giai đoạn 1. Chưa hoàn tất giai đoạn 1; chưa đổi luồng mở nguồn/xem trước/xuất mặc định.

## Đã tích hợp

- `src-tauri/src/draft.rs` ghi `draft.json` phiên bản 1 cạnh các file việc hiện có, qua cơ chế ghi an toàn của đợt 1. Manifest lưu nguồn, SHA-256 nguồn, cấu hình hình/tiếng, dịch vụ/model (không có key), hash các file đầu vào, revision, ID/slot câu, lời/mốc hiện tại và các dấu xác nhận WAV đã chốt.
- Trước khi chạy pipeline, đối chiếu nội dung nguồn với manifest. Thay nội dung file ngay tại đường dẫn cũ cũng bị phát hiện; báo chọn folder kết quả khác/đổi tên nguồn, giữ dữ liệu cũ. Đường mở lại kết quả khớp cấu hình cũng kiểm tra nguồn nếu đã có manifest. Thư viện vẫn mở bản đã xuất độc lập.
- Đầu lượt chạy tính SHA-256 model/config giọng đang chọn. Tạo khóa giọng theo nguồn, ID câu, ngôn ngữ, giọng/model giọng, lời thực tế sau bảng cách đọc, cấu hình Không dịch/thuật ngữ và phụ thuộc lời chép/tên/thuật ngữ. Đổi kiểu chữ hoặc riêng mốc không đổi khóa tiếng; thay đổi có thể ảnh hưởng toàn bộ lời được vô hiệu hóa thận trọng trên toàn phần phụ thuộc.
- `job::write_json` cập nhật manifest sau khi chốt file cấu hình/lời/mốc liên quan. Revision chỉ tăng khi dữ liệu/cấu hình thay đổi; đọc hoặc chốt thêm một WAV không tự tăng revision nội dung. Có revision lời đọc riêng từng câu để không hủy TTS câu khác khi lô dịch kế tiếp được lưu.
- Yêu cầu đọc lấy một vé gồm ID/slot câu, khóa lời đọc, revision lời đọc và mã lượt chạy. `tts::save_line_wav` kiểm tra vé dưới khóa nháp, ghi WAV xong mới ghi dấu xác nhận chứa SHA-256 WAV. Phản hồi từ lượt chạy cũ hoặc câu đã sửa rồi đổi lại không được ghi; sửa mốc/kiểu chữ vẫn cho nhận giọng đang xử lý nếu lời đọc không đổi.
- Piper native ghi nhận các WAV hoàn chỉnh khi tiến trình trả về, kể cả các câu đã xong trước khi bị hủy; WAV bị cụt không được xác nhận và được bỏ khỏi lô mới tạo để thử lại. Đường trong cửa sổ chốt sau từng câu. Mẫu thử giọng và file trong `watch/` giữ hợp đồng cũ.
- Giữ ID/slot của `editing.json`; không đổi ID/chia lại câu khi nhập việc cũ. Tách/hoàn tác không làm mất dấu xác nhận hoặc đổi file của câu khác.
- Lệnh IPC `draft_state(path)` trả manifest hiện tại, danh sách ID có tiếng đã xác minh, WAV có sẵn nhưng chưa xác minh và các vùng nghe đã sẵn sàng theo mốc hiện tại. Các vùng được sắp xếp trước khi gộp, kể cả khi người dùng đã chuyển câu qua nhau trên timeline. Lệnh chỉ đọc, không tạo manifest, không đổi revision trên đĩa, không gọi AI/mạng và không đánh dấu có bản xuất.
- Trạng thái nháp có `preparing`, `exporting`, `idle`, tách khỏi `published` của preview. `idle` chỉ có nghĩa pipeline đã trả về, không xác nhận xuất thành công. Sau đóng cưỡng bức, trạng thái ghi cuối có thể còn `preparing`/`exporting`; trạng thái việc đang chạy của app vẫn do `DubControl` sở hữu.

## Nhập việc cũ và phục hồi

Lượt chạy pipeline đầu tiên tạo thêm manifest, không sửa định dạng hoặc chuyển chỗ các file nền, không ghi đè bản xuất chỉ vì nhập nháp. Lời/mốc đọc qua validation hiện có. WAV cũ không tự được cấp dấu xác nhận: tồn tại/đọc được chưa chứng minh nó thuộc đúng lời/giọng. Các file nền còn nguyên nếu tạo manifest lỗi; có thể kiểm tra rồi chạy lại. Manifest hỏng/khác phiên bản báo lỗi rõ và được giữ nguyên, không bị thay bằng nháp rỗng.

Ghi JSON nền và ghi manifest là hai lần commit riêng. Nếu dừng giữa hai lần, lần quan sát sau đối chiếu lại hash/câu từ dữ liệu nền. Nếu WAV đã ghi nhưng chưa ghi được dấu xác nhận, nó chưa được báo là sẵn sàng. Chưa có giao dịch nhiều file hoặc nghiệm thu mất điện.

## Phạm vi còn lại

Cập nhật sau đợt này: [đợt 3 đã nối cache chữ/giọng và quyền sở hữu tác vụ vào pipeline](trien-khai-khi-can-dot-3-2026-10-29.md). Phạm vi bên dưới mô tả trạng thái tại cuối đợt 2.

Manifest hiện bảo vệ các lần tạo WAV mới và cung cấp trạng thái đã xác minh. Chưa thay đồng loạt quyết định tái sử dụng cache của pipeline cũ: những nhánh `file_ready`, cache preview và việc cũ vẫn theo hợp đồng trước đây. Chưa có dấu xác nhận bản dịch theo đủ cấu hình/phụ thuộc; hash/file lời trong manifest hiện mô tả bản nháp đang lưu, không chứng nhận mọi bản dịch đã hợp lệ theo dịch vụ/tên/thuật ngữ mới.

Chưa có bộ điều phối chung chuẩn bị/nghe/xuất, nạp theo vùng, ưu tiên vùng tua, chuyển quyền từ preview sang xuất hoặc UI dùng `draft_state`. Chưa hoàn tất cơ chế khóa/giao dịch sửa nháp khi xuất. Các giai đoạn 2–5 vẫn theo kế hoạch gốc. Không quảng bá bản nháp này như chế độ xử lý khi cần đã hoàn thiện.

Quét SHA-256 nguồn/model giọng thêm lượt đọc đĩa ở đầu pipeline; chưa đo tác động trên nguồn dài. `draft_state` kiểm tra JSON/WAV, không quét lại toàn nguồn/model mỗi lần đọc; dấu nguồn/model được xác lập lại khi bắt đầu lượt pipeline. Chưa kiểm chứng app/model/dịch vụ thật hoặc đóng app đột ngột trong luồng mới. Dữ liệu audio/video không được gửi đi.

## Kiểm chứng

Bổ sung kiểm tra lưu từng phần/mở lại, nhập WAV cũ chưa xác minh, phản hồi revision/lượt chạy cũ, sửa riêng câu, sửa rồi đổi lại, đổi kiểu chữ/mốc, đổi giọng/cách đọc/tên, nguồn bị thay nội dung, manifest khác phiên bản, WAV bị sửa/cụt, tách/hoàn tác, trạng thái chuẩn bị/xuất/nhàn và bảo toàn bản xuất cũ. Có kiểm tra riêng đường mở lại kết quả để không bỏ qua dấu nguồn.

`npm run verify:local` qua sáu nhóm: frontend (95 test), hàng đợi, playback, kiểm tra gói Windows, build frontend và Rust offline (306 qua, 4 bỏ qua ở lượt kiểm tra đó). Báo cáo: [report.json](../.bench-asr/verify-1793252442958/report.json). Sau đó bổ sung chốt kiểm tra nguồn trên đường mở kết quả và chạy lại toàn bộ `cargo test --manifest-path src-tauri/Cargo.toml --offline --lib`: 307 qua, 0 lỗi, 4 bỏ qua.
