# Mốc C — Tách/gộp câu và định danh ổn định

Ngày: 28/10/2026. Đã triển khai mốc C. Chưa triển khai lịch sử các bản xuất của mốc D. Giữ Tauri, không đổi tên app, không tải thêm công cụ/model, không commit.

## Thao tác

Trong video kết quả, mở **Chỉnh thời gian câu**, chọn câu và đặt vị trí phát:

- **Tách tại vị trí phát** mở hai phần lời gốc/lời đọc. Phần gợi ý chia theo thời gian phải được xem lại; có thể sửa chữ và mốc tách. Mỗi phần cần ít nhất 0,08 giây, lời không rỗng, tối đa 4.000 ký tự mỗi ô.
- **Gộp với câu sau** chỉ nhận hai câu liền nhau không chồng mốc (dung sai 5 ms), khoảng nghỉ tối đa 2 giây. Khoảng mới bao gồm khoảng nghỉ đó. App không tự nhận diện người nói; form ghi rõ để người dùng cân nhắc trước khi gộp.
- Phải đánh dấu đã xem lại rồi mới bấm **Tách/Gộp và xuất lại**. Hủy form không lưu dữ liệu. Có nháp lời/mốc chưa xuất thì cần Xuất hoặc Bỏ sửa trước, tránh mất nháp khi đổi số câu.
- **Hoàn tác tách/gộp** lưu tối đa 20 thao tác qua lần mở lại. Hoàn tác chỉ thay các câu thuộc thao tác đó, giữ sửa ở câu khác. Nếu chính câu cần hoàn tác đã sửa lời/mốc tiếp thì nút báo không thể tự hoàn tác đè lên; backend cũng từ chối nếu gọi trực tiếp.

Tách/gộp và hoàn tác đều xuất lại kết quả. Khi chỉ khôi phục câu có tiếng còn phù hợp, dùng lại WAV. Nếu cách đọc/giọng/danh sách tên thay đổi so với snapshot thì câu phục hồi nhận file tiếng mới; tên đã xác nhận được áp dụng để không khôi phục lại tên sai. Phần đọc giọng vẫn theo engine và giọng người dùng đã chọn.

## Cấu trúc và liên kết tiếng

`editing.json` phiên bản 1 là lớp biên tập, chứa ID câu, liên kết tới chỉ số câu gốc, mốc/chữ hiện tại, số file tiếng và lịch sử thao tác. Lần đầu tách/gộp tạo ID tương thích cho các câu cũ. Không đổi tên hàng loạt WAV, không ghi ngược vào lời Whisper.

Ví dụ thử tách câu thứ hai của một việc có ba câu:

| Câu đang hiển thị | ID | WAV | Liên kết câu gốc |
| --- | --- | --- | --- |
| 1 | `cue-000001` | `001.wav` | 1 |
| 2, phần đầu | `edit-4` | `004.wav` | 2 |
| 3, phần sau | `edit-5` | `005.wav` | 2 |
| 4 | `cue-000003` | `003.wav` | 3 |

`002.wav` vẫn giữ để hoàn tác; câu cuối không bị đọc nhầm `004.wav` chỉ vì nay đứng thứ tư. Số file mới tăng dần và bỏ qua file đã tồn tại. ID hiện tại duy nhất, số file tiếng duy nhất và mọi liên kết nguồn được kiểm tra trước khi dùng. Ánh xạ được cache theo metadata của cả lớp biên tập và bản dịch gốc, tối đa 32 việc trong bộ nhớ; lưu/khôi phục làm hết hiệu lực cache.

`translation.json`, `lines.json` và `transcript.json` giữ dữ liệu nền. Bản đọc hiệu lực áp lớp `timing.json` của mốc B trước, rồi dùng chuỗi câu trong `editing.json`. Các đường đọc lời, căn tiếng, tạo giọng, SRT, đoạn thử, đổi cách đọc và sửa tên đều dùng đúng ánh xạ hiện tại. Thông tin draft tính các câu đang biên tập và chỉ đếm WAV thuộc các câu đó.

Đổi riêng chữ/mốc sau tách/gộp phải mang ID và phiên bản dữ liệu hiện tại. Các lệnh sửa cũ không có ID vẫn dùng được với việc chưa có lớp cấu trúc; việc đã tách/gộp sẽ yêu cầu mở lại thay vì đoán theo số thứ tự.

## Chạy tiếp và tương thích

- Không có `editing.json`: đường cũ, WAV số thứ tự cũ và retake cũ vẫn dùng được.
- Có lớp biên tập: chạy tiếp dùng các câu đã chốt, không chạy lại Whisper, tên, thuật ngữ hoặc dịch; chỉ bổ sung tiếng còn thiếu rồi dựng phần sau. Không tự rút gọn lại lời đã biên tập bằng AI. Đổi khung/âm lượng vẫn giữ các câu và file tiếng còn dùng được.
- Đổi ngôn ngữ/lựa chọn dịch, yêu cầu dịch lại hoặc chép lời lại: cất lớp cấu trúc cùng kết quả cũ vào `cu/` trước khi trở về chuỗi gốc. Không tự dùng ID của chuỗi cũ cho bản dịch/chuỗi mới.
- File cấu trúc hỏng, phiên bản không hỗ trợ, ID trùng hoặc không khớp nguồn: báo rõ; không lùi về WAV theo số thứ tự. Cache legacy chưa hoàn chỉnh khi đổi khung vẫn được xử lý khoan dung như trước; lớp cấu trúc mới luôn phải hợp lệ.
- Xem trước dịch trực tiếp vẫn dùng đường mốc gốc. Với việc đã tách/gộp, đường này báo rõ để dùng bản kết quả, dựng đoạn thử hoặc Xuất lại; không trộn lời gốc với WAV của chuỗi đã đổi. Audio không có nút tách/gộp trực tiếp.

## Hủy và lỗi

Manifest retake `_undo` bổ sung trạng thái tồn tại của lớp cấu trúc. Backup/khôi phục WAV dùng số file vật lý, không dùng vị trí câu. Ý định chuyển từng kết quả được ghi trước khi chuyển; metadata chỉ chốt cùng bản xuất thành công.

Lỗi hoặc hủy khôi phục lớp cũ, bản dịch nền, WAV bị thay và kết quả. WAV mới sinh thuộc lượt hủy được dọn, WAV cha và các câu không liên quan vẫn giữ. Cache trộn/frame vừa sinh cũng được trả/dọn theo trạng thái trước lượt chạy. Manifest retake trước mốc C vẫn đọc được qua trường mặc định.

## Kiểm chứng

Đã chạy bộ hồi quy Windows `npm run verify:local`; báo cáo cuối tại `.bench-asr/verify-1793183832735/report.json`. **88 kiểm thử frontend, 3 kiểm thử hàng đợi, 9 kiểm thử kiểm tra gói Windows, TypeScript/Vite build và 269 kiểm thử Rust đều qua**; Rust có 3 mục bỏ qua theo cấu hình có sẵn. Lượt sửa cuối Rust riêng cũng có log `.bench-asr/structure-20261028/rust-final.log`.

Kiểm thử mới gồm tách đầu/giữa/cuối; gộp rồi hoàn tác hai lần; đọc lại file đã lưu; giữ sửa độc lập; từ chối ghi đè sửa mới; tên/cách đọc đổi rồi hoàn tác; metadata hỏng hoặc nguồn không khớp; trở về ánh xạ legacy khi chủ động reset; phục hồi file tiếng mới và sửa đúng file vật lý sau khi chỉ số bị dịch. Gợi ý chia chữ có kiểm tra Unicode/emoji và yêu cầu điền phần còn thiếu thay vì tự bịa lời.

Lượt hồi quy đầu phát hiện đổi khung của một việc legacy có cache chưa hoàn chỉnh bị chặn bởi phép đọc nghiêm ngặt mới. Đã khôi phục cách xử lý cũ cho việc không có lớp cấu trúc, giữ kiểm tra nghiêm ngặt cho việc có ánh xạ, rồi chạy lại kiểm thử.

### Tauri, Piper và FFmpeg thật

Dữ liệu thử là 13,6 giây đầu của video nghiệm thu sẵn có, thu nhỏ nguồn thành 480×270, ba câu/WAV cũ. Tất cả ở bản sao `.bench-asr/structure-20261028/`; không sửa folder nguồn hoặc cache nghiệm thu trước. Dùng giọng đã chọn `ngochuyennew`, Piper có sẵn và bridge TTS thật. Không gọi dịch hoặc Whisper, không tải tài nguyên.

1. Yêu cầu tách với phiên bản cũ bị từ chối, hash kết quả giữ nguyên.
2. Hủy ở bước đọc giọng trả lại chuỗi cũ, WAV và kết quả; không còn `editing.json` dở hoặc `_undo`. Sau đó nạp lại WebView để bắt đầu lượt kiểm tra tiếp.
3. Thao tác form thật: xem lại bốn ô lời gốc/lời đọc, nút xuất chưa bật khi chưa xác nhận; tách câu thứ hai tại giây 6,800 và xuất thành công. Chỉ hai chỉ số mới được gửi đọc; ba WAV cũ giữ nguyên SHA-256. ID cuối vẫn `cue-000003`, trỏ tới `003.wav` dù nay đứng thứ tư.
4. Sửa mốc câu cuối sang 8,600 giây sau khi tách: đúng ID, không đọc lại tiếng.
5. Gộp hai phần vừa tách: chỉ đọc một câu mới. Hoàn tác gộp rồi hoàn tác tách không gọi TTS; dùng lại đúng WAV cha/con đã giữ, mốc sửa ở câu cuối còn nguyên.
6. Tách tiếp câu đầu, lưu London → Luân Đôn và sửa tên: chỉ các câu khớp tên được đọc lại; câu không liên quan giữ file tiếng. Lớp nguồn vẫn giữ liên kết tới câu gốc.
7. Thử hoàn tác sau sửa tên ở chính câu đó: backend từ chối và giữ nguyên hash metadata/kết quả; không ghi đè lời mới.
8. Cất bản xuất của fixture rồi gọi `start_dub` để dựng lại: dùng bốn câu đã biên tập và đúng WAV, không chép lời/dịch/đọc lại; `translation.json`, `lines.json`, `transcript.json` giữ hash ban đầu.

Chi tiết ID, file tiếng và hash: `.bench-asr/structure-20261028/report.json`. Video cuối 270×480, H.264/AAC, dài khoảng 13,633333 giây; FFmpeg giải mã toàn bộ không lỗi. Đã xem [khung hình sau chạy tiếp](../.bench-asr/structure-20261028/resume-frame.png), có lời mới “Đây không phải Luân Đôn.”; [form xem lại](../.bench-asr/structure-20261028/split-review.png) đã được thử trong WebView.

Harness chỉ gắn tạm cho lượt thử; `src/main.tsx` đã khôi phục đúng SHA-256 `F79D82A9224C2BE86159406DA47ED4A888A91527D42B4A1C5A75558318FA74CB`. Bundle production được kiểm tra không có harness, đường test hoặc biến QA. Không thay key, giọng được lưu hay hàng đợi của người dùng.

## Giới hạn

Không có nhận diện người nói, căn từng từ hoặc AI tự phân chia nội dung. Liên kết nguồn là câu gốc, không phải vị trí từng ký tự. Mỗi thao tác tạo bản xuất mới tại vị trí hiện tại; lịch sử các video đã xuất thuộc mốc D, khác lịch sử hoàn tác cấu trúc.

Giữ WAV cha/con để hoàn tác nên thư mục tiếng có thể chứa file không thuộc chuỗi hiện tại; chưa tự dọn các file đó khi lịch sử vượt 20 thao tác. Chưa đánh giá hiệu năng trên việc 10.000 câu, mọi giọng/ngôn ngữ hoặc mọi dịch vụ. Lượt này không xác nhận giọng tự nhiên chỉ bằng việc xuất thành công, không nghiệm thu tải link thật hoặc installer và chưa đóng gói bản phát hành mới.
