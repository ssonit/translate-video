# Mốc B — Chỉnh thời gian câu

Ngày: 28/10/2026. Đã triển khai chỉnh mốc, sóng âm và nghe lặp cho video đã xuất. Chưa triển khai tách/gộp câu, định danh ổn định của mốc C hoặc lịch sử bản sửa của mốc D. Không commit.

## Cách dùng

1. Mở video kết quả, bấm **Chỉnh thời gian câu** cạnh danh sách lời thoại.
2. Chọn câu; kéo đầu/cuối hoặc thân thanh để đổi mốc, hoặc nhập giây và bấm **Giữ mốc nháp**. Có các cửa sổ 10/20/60 giây và nút dịch vùng xem.
3. Dùng **Nghe lặp đoạn đang xem** để kiểm tra bản xuất hiện tại quanh khoảng câu, với đệm 0,25 giây. Mốc nháp chưa thay nội dung hình/tiếng đang phát; giao diện ghi rõ điều này.
4. **Hoàn tác mốc** giữ các sửa lời độc lập. **Bỏ sửa** ở cuối danh sách bỏ các thay đổi chưa xuất và xóa lịch sử hoàn tác trong phiên.
5. Bấm **Xuất lại · N câu đã sửa** để áp dụng. Chỉ đổi mốc giữ WAV từng câu, tính lại căn tiếng/trộn/phụ đề và xuất video. Có sửa lời hoặc chọn Đọc lại thì mới tạo lại tiếng theo đường retake hiện có.

Chọn thời gian trong video và khoảng dài ít nhất 0,08 giây. Không di chuyển qua thứ tự câu bên cạnh hoặc tăng phần chồng với câu khác (dung sai 5 ms cho mốc số lẻ). Phần chồng có sẵn có thể giữ/giảm, không tự dịch câu bên cạnh. Nghe audio liền mạch không có phần chỉnh mốc video; backend cũng từ chối yêu cầu này.

## Dữ liệu và cache

- `timing.json` phiên bản 1 lưu các khoảng gốc theo thứ tự và mốc đã sửa theo chỉ số. Giữ nguyên `lines.json` của Whisper, `transcript.json` và mốc gốc trong `translation.json`. Chỉ lưu lại chữ dịch khi chữ thực sự đổi; chỉnh riêng mốc không chuẩn hóa lại JSON hoặc xóa khoảng trắng trong bản dịch.
- Mốc được áp dụng khi đọc lời video, xuất/chạy tiếp pipeline, dựng đoạn thử, sửa lời/tên và tạo lại SRT để ghi chữ. Khi bước nói rút gọn lời, phần ghi chữ vẫn trả thời gian về mốc gốc trước khi lưu, tránh áp lớp mốc hai lần. Việc có lớp chỉnh mốc giữ lời đã chốt khi pipeline dựng lại, không tự rút gọn bằng AI theo khoảng thời gian mới.
- Bản xem trước dịch trực tiếp giữ đường riêng như kế hoạch. Audio liền mạch tiếp tục dùng mốc audio riêng. Bộ đếm tiến độ draft và đổi cách đọc chỉ cần chữ nên vẫn đọc dữ liệu gốc.
- Không có `timing.json` thì dùng cách đọc việc cũ. Không khớp khoảng gốc hoặc không đọc được phiên bản thì báo rõ, giữ file để kiểm tra, không đoán ánh xạ WAV. Chép lời từ đầu cất lớp mốc cũ vào `cu/timing-<mốc>/` trước khi tạo chuỗi câu mới.
- Mỗi tập lời trả về kèm SHA-256 phiên bản dữ liệu liên quan. Yêu cầu chỉnh thời gian bắt buộc mang phiên bản đó; thay đổi lời, tên hoặc cấu hình trong lúc màn cũ còn mở sẽ bị từ chối. Các lệnh sửa chữ cũ không gửi mốc vẫn đọc được.
- Sóng âm đọc trên máy bằng FFmpeg, lấy khoảng 2.400 giá trị đỉnh. Ưu tiên file nguồn chứa tiếng, kể cả khi việc tải link giữ tiếng riêng với hình; thiếu nguồn mới dùng bản xuất. Không đưa cả PCM vào cửa sổ. Metadata có trước, sóng âm tải sau; mỗi lần xử lý tối đa 30 giây và thiếu sóng âm vẫn nhập mốc được.
- Cache `waveform.json` kiểm tra phiên bản, đường nguồn, kích thước, thời điểm sửa và độ dài. Chỉnh mốc không làm lấy lại sóng âm nếu nguồn còn khớp. Đây là hình gợi ý biên độ, không phải căn chính xác từng từ.

## Giao dịch và khôi phục

Retake mở rộng manifest `_undo` để giữ lớp mốc cũ và các cache phụ thuộc. Ghi ý định vào manifest trước từng lần chuyển file. Khi lỗi/hủy, trả lại bản dịch, lớp mốc, kết quả và các WAV đã thay; dọn bản trộn/cache vừa sinh nếu trước đó chưa có. Cách này tránh dùng bản trộn của mốc mới sau khi đã trả về mốc cũ.

Đổi riêng thời gian bỏ qua kiểm tra/nạp giọng và bỏ qua TTS hoàn toàn. Nếu WAV cần ghép bị thiếu thì báo lỗi và phục hồi, không tự đọc lại câu khác. Bản mới chỉ được chốt khi ghi được cờ commit; lỗi ghi cờ phải phục hồi thay vì báo thành công. Manifest retake cũ vẫn đọc được qua trường mặc định. Việc đang chạy hoặc ghi chữ không nhận thao tác sửa mới.

Màn kết quả cũng xóa thông báo lỗi cũ khi thay bản xuất và gom nút Sửa bản xuất vào hàng lựa chọn để bố cục cạnh video không phát sinh hàng lưới ngoài dự kiến.

## Kiểm chứng

Lượt `npm run verify:local` sau khi tháo harness: **86 kiểm thử frontend, 3 kiểm thử hàng đợi, 9 kiểm thử kiểm tra gói Windows, TypeScript/Vite build và Rust đều qua**. Báo cáo: `.bench-asr/verify-1793181204314/report.json`. Lượt này có 259 Rust qua; sau bổ sung kiểm tra sóng âm/cache/video im lặng và chỉnh sửa cuối, chạy lại toàn bộ Rust: **260 qua, 0 lỗi, 3 bỏ qua theo cấu hình có sẵn**. Log ở `.bench-asr/timing-20261028/rust-final.log`.

Kiểm thử mới kiểm tra lớp mốc sau đổi chữ, giữ mốc gốc khi lưu lời, file cũ/hỏng/phiên bản lạ, giới hạn thời gian và phần chồng, sửa nguyên nhóm, phiên bản stale, phục hồi hai lần chỉnh liên tiếp và giữ đúng từng WAV. JSON có định dạng/khoảng trắng cũ phải giữ nguyên byte khi chỉ chỉnh mốc.

### IPC và giao diện Tauri thật

Dùng bản sao độc lập tại `.bench-asr/timing-20261028/recheck/hometown-75s/`, từ video và cache 16 câu đã có. Không sửa folder nghiệm thu gốc, không gọi Whisper, dịch vụ dịch hoặc TTS. Harness gắn tạm vào `main.tsx` chỉ cho lượt kiểm tra rồi được tháo; entry được khôi phục đúng SHA-256 `F79D82A9224C2BE86159406DA47ED4A888A91527D42B4A1C5A75558318FA74CB`. Không đưa mã kiểm thử vào bundle production, không thay lựa chọn/key hoặc localStorage hàng đợi của người dùng.

- Đọc độ dài 75,033333 giây và 2.391 giá trị sóng âm nguồn qua IPC.
- Phiên bản stale, mốc vượt video và yêu cầu tạo thêm chồng câu đều lỗi; bản xuất giữ nguyên hash.
- Sửa câu thứ tư thành 13,620–17,320 giây, xuất đầy đủ thành công. `dub_lines` đọc lại đúng mốc mới. `translation.json`, `lines.json`, `transcript.json` và toàn bộ 16 WAV giữ nguyên SHA-256; sự kiện speak là skipped và không có `tts-request`.
- Hủy qua `cancel_dub` khi bước fit bắt đầu ở lượt sửa tiếp: khôi phục đúng hash video và `timing.json` vừa chốt; không có `.partial.mp4` hoặc `_undo` còn sót.
- Trong WebView thật: chọn câu, nhập mốc, giữ nháp, thấy nút xuất, kéo đầu câu bằng pointer và hoàn tác. Nghe lặp đã phát qua cuối khoảng rồi quay lại; dừng không để thông báo play bị hủy giả. Hoàn tác mốc không xóa lời đang sửa, Bỏ sửa xóa cả lịch sử hoàn tác.
- Xem ảnh ở 1080×800: video dọc bên trái, phần chỉnh mốc/lời ở bên phải. Ở 600×800: xếp dọc, phần chỉnh có cuộn và danh sách vẫn còn chỗ; không tràn ngang. Video giải mã được và không còn thông báo media lỗi cũ.

Log/hash trong `.bench-asr/timing-20261028/report.json`; kiểm tra UI cuối trong `ui-report.json`. Ảnh: [khung rộng](../.bench-asr/timing-20261028/timeline-wide.png), [khung hẹp](../.bench-asr/timing-20261028/timeline-narrow.png). Đây là dữ liệu local được Git bỏ qua.

Lượt thử đầu phát hiện việc ghi lại JSON bản dịch dù chỉ đổi mốc; đã sửa để không ghi khi chữ không đổi và kiểm tra lại trên bản sao mới. Không tính lượt đầu là đạt giữ nguyên byte.

## Giới hạn

Nháp và hoàn tác chỉ ở trong phiên đang xem, chưa lưu nháp qua lần đóng app. Nghe lặp phát bản xuất hiện tại, không giả lập tiếng/phụ đề đã dịch sang mốc nháp; cần Xuất lại để xem bản mới. Thuật toán căn tiếng vẫn có thể mượn khoảng nghỉ/tăng tốc/cắt như trước và phát cảnh báo; không hứa giọng luôn bắt đầu đúng từng mili giây người dùng nhập.

Chưa có tách/gộp câu. Sóng âm và ngắt chữ không phải căn từ theo âm thanh. Đợt này không xác nhận thêm chất lượng dịch/giọng, nguồn link thật, mọi ngôn ngữ hoặc installer Windows; chưa đóng gói bản phát hành mới.
