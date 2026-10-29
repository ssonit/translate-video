# Nháp, preview theo vùng và xuất theo yêu cầu — 29/10/2026

Tiếp nối ba đợt nền: các lệnh nháp mới và màn `DraftStudio` đã tách thao tác mở nguồn, lấy lời gốc, lưu sửa, nghe câu, dựng đoạn thử và xuất đầy đủ. Chạy toàn bộ/hàng đợi và preview cũ vẫn giữ đường xử lý tương thích.

## Các ranh giới xử lý

| Thao tác | Đường xử lý | Không thực hiện trong thao tác này |
| --- | --- | --- |
| Mở nguồn | `open_draft`, tải theo đường cũ nếu là link, ffprobe, đọc nháp | Whisper, dịch, TTS, ghép |
| Lấy lời gốc | `prepare_draft`, phụ đề/Whisper và ngắt câu | Dịch, TTS, ghép |
| Xem bản dịch | `set_draft_region` + `prepare_draft_region` | TTS, ghép |
| Nghe câu/vùng | Cùng bộ điều phối vùng, WAV có receipt, `timeline::place_all` | `dub.wav`, ghép toàn video |
| Lưu lời/mốc | `save_draft`, `structure_draft` | AI, TTS, ghép |
| Lưu cấu hình | `configure_draft`, journal cấu hình | AI, TTS, ghép |
| Dựng đoạn thử | `sample_draft`, vùng thử + câu liền kề | Hoàn tất mọi WAV của nguồn |
| Xuất | `export_draft`, chuyển quyền sang pipeline hiện có | Không phụ thuộc preview đã xong |

`Mở nguồn & sửa nháp` là thao tác riêng cạnh `Chạy toàn bộ`. Nút Xuất nằm ngoài phần cuộn của màn nháp. Thẻ Chạy tiếp còn thông tin tiến độ và video gốc, thêm nút mở nháp. Nháp mở lại được khi chưa có file kết quả.

## Quyền sở hữu và nạp trước

- Mỗi cửa sổ nháp có mã riêng. Client chỉ giữ một IPC đang chạy và một ý định mới nhất; tua liên tiếp thay vị trí chờ thay vì xếp thêm worker. Đóng cửa sổ cũ không được xóa ý định/hủy tác vụ của cửa sổ mới.
- Phần đệm mặc định 30 giây; backend từ chối vùng lớn hơn 60 giây. Một lô tối đa sáu câu dịch, TTS từng câu. Kiểm tra lại ý định trước lô/câu kế tiếp. Tạm dừng cho phép phần đang thực hiện chốt vào cache, rồi ngừng nạp thêm.
- Sự kiện `draft-region-progress` công bố chữ sau lô dịch và WAV sau từng câu. Có thể bắt đầu phát câu đầu trước khi vùng yêu cầu hoàn tất.
- Preview và đoạn thử cùng dùng WAV đã xác minh và quy tắc `timeline::place_all`. Khi thiếu ngữ cảnh phía trước, lịch nghe mang trạng thái tạm tính. WAV cũ còn trên đĩa nhưng sai receipt không được ảnh hưởng lịch đoạn thử.
- Lưu, cấu hình, nghe câu và xuất đều chờ request preview trả quyền trước. Hủy TTS thu hồi bridge; chỉ worker trả về mới nhả lease.

## Dữ liệu và tương thích

- `editing.json` giữ ID/slot ổn định, lời gốc, lời đọc, mốc, provenance và undo. Canonical `translation.json` không bị ghi đè chỉ vì lưu nháp. Tách/gộp dùng slot mới; hoàn tác không tái cấp ID đã dùng.
- Thay lời làm cũ receipt tương ứng; đổi riêng mốc giữ WAV và khóa việc AI rút gọn câu đó lúc xuất. Thay giọng không xóa lời. Thay kiểu chữ không gọi dịch/TTS. Đổi ngôn ngữ/Không dịch xóa lời phụ thuộc trong nháp, giữ nguồn/ID và bản sao cấu hình cũ.
- Cấu hình có journal `draft-config-pending.json` và bản sao phục hồi trong `cache-stale/`. Mở lại hoàn tất giao dịch metadata dở trước khi xử lý tiếp.
- Bản xuất có dấu revision riêng `draft-export.json`, không dùng trạng thái preview làm bằng chứng đã xuất. Bản trước vẫn mở được trong khi sửa; khi xuất bản mới, lịch sử hiện có cất bản cũ.
- Lời cũ không receipt không được tự nhận là nội dung đã được người dùng duyệt. Giữ canonical cũ để phục hồi, báo rõ trước khi dịch lại vùng cần dùng. Cấu trúc biên tập cũ vẫn đọc bằng ID/slot đã có.
- Metadata tải mới được gắn link sau khi kiểm tra đúng đường dẫn media; không đổi locator của manifest cũ. Folder nghe trùng tên nhưng thuộc link khác bị từ chối trước khi ghi.

## Xuất chính thức

Xuất chốt cấu hình, khóa sửa trong khi xử lý, lấy phần ASR còn thiếu, duyệt tên, lấy thuật ngữ nếu bật, hoàn tất lời/giọng còn thiếu rồi căn toàn bộ. Tên/thuật ngữ đổi sẽ vô hiệu hóa lời tự sinh phụ thuộc; nội dung người dùng sửa được giữ và áp dụng quy tắc tên đã duyệt. Chỉ phụ đề không TTS/fit, audio không dựng hình. Giữ đường NVENC/CPU dự phòng, partial, kiểm tra media và lịch sử bản xuất cũ.

Kiểm tra công cụ của nháp dùng đúng provider, model, ngôn ngữ, giọng và Không dịch của nháp, không khóa nhầm theo lựa chọn đang soạn ở nguồn khác. Đồ nghề chưa có kết quả vẫn không khóa; Rust kiểm tra đầu lượt. DeepSeek local thiếu server báo trước khi chép lời/xuất hay cất bản cũ.

Kết quả thử và giới hạn: [biên bản nghiệm thu](nghiem-thu-khi-can-2026-10-29.md).
