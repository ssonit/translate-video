# Giảm câu vụn và cảnh báo cắt giọng — 31/10/2026

## Thay đổi

- `speech_plan.rs` và `media.rs`: câu nguồn có khoảng trắng được gom/chia theo cụm lời thay vì ngưỡng 75 ký tự chung. Giới hạn cụm 8 giây, khoảng hở 0,45 giây, tối đa khoảng 220 ký tự cho văn bản có ít nhất bốn từ và không thuộc CJK; tiếng Trung/Nhật/Hàn vẫn theo ngưỡng 75. Câu dài vẫn chia; số phần có giới hạn để tránh vòng lặp không giới hạn với mốc lỗi.
- Chỉ áp dụng lúc tạo `lines.json` mới. File đã lưu và ID/mốc/lời biên tập không bị tự chia lại. Luồng preview cũ trong `watch.rs` vẫn giữ cách chia riêng, không được dùng làm bằng chứng cùng cấu trúc với nháp mới.
- `demand.rs`: `View.speechWarnings` trả ID câu, số giây có nguy cơ bị cắt và cờ tạm tính. Tính từ thời lượng trong snapshot WAV đã xác minh và `timeline::place_all`; không đọc WAV lần hai hoặc dùng lại fit-report đã cũ. Thiếu WAV phía trước làm kết quả sau đó tạm tính. Chế độ chỉ chữ/audio liền mạch không có cảnh báo căn giọng video.
- `DraftStudio`: hiển thị tổng câu/giây có nguy cơ bị cắt ngay trên thanh Xuất, đánh dấu từng câu và có nút **Đến câu cần kiểm tra**. Thay đổi chưa lưu ẩn cảnh báo dựa trên nháp cũ, lưu lại mới cập nhật. Cảnh báo không chứng minh các câu chưa có WAV sẽ vừa; không tự tạo toàn bộ WAV chỉ để dự đoán.
- Frontend chấp nhận thiếu trường mới khi đọc dữ liệu cũ. Không đổi schema file nháp, không đổi MAX_SPEED=1.08 hoặc chính sách mượn khoảng trống, không bỏ từ để làm đẹp báo cáo.

Điều kiện rút gọn câu tự dịch trong nháp đã có ở code hiện tại; không sửa điều kiện này sau khi đối chiếu trực tiếp. Câu authored/keep_speech tiếp tục được bảo vệ bởi đường đọc hiện có.

## Nguồn và phép đo mới

Bằng chứng trong `.bench-asr/speech-quality-20261031/`. Cùng video thật 60 giây từ mốc 40–100 của nguồn Grok 105 phút, transcript mới từ lượt ASR toàn nguồn ở đợt trước. Không ASR lại để thay đầu vào. Baseline ở `.bench-asr/continue-20261031/gpu-minute/` được đọc để so, không sửa.

Chạy binary Tauri **debug mới** với frontend Vite/WebView thật; đây chưa phải kiểm chứng EXE release mới hoặc bộ cài. Dùng DeepSeek local `deepseek-chat`, giọng `ngochuyennew`, không thuật ngữ. Người dùng đã cho phép bật DeepSeek để thử trong phiên; bật bằng lệnh app, tắt lại sau thử. Không tải voice/model, không gửi audio/video lên mạng và không đổi cấu hình toàn app. Driver và profile riêng, không đưa hook vào mã sản phẩm.

Từ cùng transcript, câu nguồn giảm **34 → 12**. So toàn bộ token nguồn trước/sau: giữ đủ và đúng thứ tự. File việc cũ 34 câu vẫn đọc được; warning mới nhận đúng 19 câu cắt từ WAV cũ.

### Bản dịch mới

Chuẩn bị vùng 0–30 giây có giọng: 28.760 ms, 8/12 WAV. Xuất phần còn thiếu và đốt chữ: 33.776 ms kể từ yêu cầu xuất (có duyệt tên tự động trong driver, không phải người duyệt). MP4 giải mã toàn file bằng FFmpeg `-xerror` đạt.

| Lượt | Câu bị cắt | Giọng không được giữ sau tăng tốc |
| --- | ---: | ---: |
| Bản trước, lời AI trước | 19/34 | 11,1137 giây |
| Gom/chia mới, lời AI mới | **2/12** | **0,8307 giây** |

Response AI và profile debug/release khác nhau giữa hai lượt; không quy toàn bộ thay đổi cho thuật toán gom câu, không dùng thời gian này công bố tăng tốc pipeline.

### Đối chứng giữ nguyên lời và cùng đường đọc

Tạo hai fixture sạch `replay-before` và `replay`, đều authored để không gọi dịch/rút gọn, tên đã confirmed rỗng. Cùng app/giọng và cùng đường đọc toàn lô, không dùng WAV preview có sẵn. Bản 34 câu giữ cấu trúc cũ; bản 12 câu nối nguyên lời theo các nhóm nguồn mới. 34 câu cũ được ánh xạ mỗi câu đúng một lần sang 12 nhóm (`replay-map.json`).

Đối chiếu lời hiệu lực trong `editing.json`: toàn bộ token tiếng Việt giữ nguyên và đúng thứ tự, không bỏ/thêm từ. Cả hai WAV được tạo mới; thay ranh giới câu làm ngữ điệu và khoảng nghỉ thay đổi, không phải tái dùng audio giống từng byte.

| Lượt đối chứng | Câu bị cắt | Giọng không được giữ |
| --- | ---: | ---: |
| 34 câu cũ, đọc lại toàn lô | 17/34 | 11,3082 giây |
| 12 nhóm mới, nguyên từ dịch cũ | **4/12** | **3,8459 giây** |

Phần giọng mất giảm khoảng **66%** trong cặp thử này. Hai MP4 cùng bản dịch mới đều giải mã thành công. Đây là một cặp lượt trên một đoạn nói tiếng Anh; chưa đại diện mọi video/ngôn ngữ/giọng và chưa được người nghe nghiệm thu ý/ngữ điệu.

## Cảnh báo trong UI

Mở bản replay 12 câu trong WebView thật: thanh cảnh báo hiện **4 câu / 3,85 giây**; nút chuyển lựa chọn từ câu đầu sang câu lỗi tiếp theo ở giây 20, đánh dấu đúng ID. Mở bản dịch mới hiện **2 câu / 0,83 giây**, khớp hai mục fit-report (câu 1 và 3).

Cửa sổ CSS 1080×760: cảnh báo ở y≈626–687, thanh Xuất ở y=687–760, nút Xuất vẫn trong viewport. Screenshot `warning.png`, `warning-new.png` đã xem lại. Đây là kiểm chứng DOM/screenshot của WebView thật qua CDP, không phải click chuột Windows bản địa. Không có hộp thoại xác nhận xuất mới hoặc tự khóa người dùng vì dự đoán tạm tính.

## Hồi quy

`npm run verify:local`: **cả sáu nhóm đạt**, gồm **104 frontend**, **343 Rust đạt / 7 ignored**, queue/playback/windows-package/build. Log mới `.bench-asr/verify-1793448053729/`. Không gọi các benchmark ignored là đã chạy trong lượt này.

Test bổ sung: giữ tiền và ngữ cảnh trong cùng cửa sổ; giữ đủ từ khi chia câu dài/CJK; warning dùng cùng placement với clip, WAV hỏng bị loại ở lần đọc sau, thiếu ngữ cảnh mang cờ tạm; frontend đọc trường cũ thiếu/giá trị không hợp lệ và không cảnh báo co giọng cho audio/chỉ chữ. `git diff --check` đạt, chỉ có cảnh báo chuẩn hóa LF/CRLF của Git.

Không sửa/push dữ liệu việc thật. Tắt DeepSeek, đóng đúng tiến trình app thử và Vite do lượt này khởi động. Chưa commit/push hoặc rebuild gói release.

## Giới hạn còn lại

Hai câu ở bản dịch mới vẫn thiếu khoảng **0,50s và 0,33s**; cảnh báo hiện rõ, chưa tự sửa mốc hoặc bỏ ý để ép về 0. Cần nghe và sửa cụ thể những câu đó. Chưa thể kết luận đạt chất lượng lồng tiếng toàn bộ, đã giữ đủ ý qua mọi request rút gọn, hoặc mốc chữ khớp khẩu hình.

Chưa áp dụng cách chia mới lên toàn nguồn 105 phút, chưa xuất toàn nháp dài từ cache thiếu, chưa thử ma trận tiếng Trung/những ngôn ngữ khác bằng video thật hoặc bộ cài Windows sạch. Dữ liệu việc cũ muốn chia theo cách mới phải chủ động chép/chia lại hoặc tạo việc mới; không được âm thầm xóa cache đã biên tập.
