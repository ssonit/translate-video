# Kế hoạch nâng cấp tiếp theo

Ngày lập: 28/10/2026. Đã triển khai phần mã của việc 1–6. Việc 7 đã có kiểm thử tự động và chuẩn bị đường dẫn/resource cho bản Windows; nghiệm thu installer trên Windows sạch còn chờ. Xem [báo cáo triển khai](nang-cap-2026-10-28.md) để phân biệt phần đã kiểm chứng và phần còn lại.

## Điểm xuất phát

Lượt tiếp mục 7 đã bổ sung kiểm tra link một video YouTube, độ đầy đủ manifest gói Windows, video xoay/đường dẫn Unicode và lỗi lưu file chỉ đọc. Có lệnh `npm run verify:local` để chạy lại; xem [báo cáo kiểm thử phát hành](kiem-thu-phat-hanh-2026-10-28.md). Những nhánh API, tải link thật và máy sạch vẫn chưa được xác nhận.

Đã có khung 9:16 / 1:1 / 16:9, nền mờ hoặc cắt khung, chỉnh kiểu phụ đề và lưu bộ cài đặt. Đã kiểm tra nguồn local 75 giây qua DeepSeek local, dừng/chạy tiếp và hàng đợi hai video. Đổi riêng kiểu chữ đã giữ nguyên video chưa có chữ, bản dịch và WAV từng câu.

Hàng đợi, sửa từng câu, sửa tên, cảnh báo câu bị cắt và chạy tiếp đã tồn tại; các việc dưới đây mở rộng những phần đó.

## Quy tắc chung

- Giữ Tauri, xử lý hình và tiếng trên máy. Không đổi tên sản phẩm, không tự tải thêm công cụ hoặc model.
- Tùy chọn mới có mặc định tương thích việc cũ, được giữ theo từng video trong hàng đợi và được lưu trong bộ cài đặt khi phù hợp.
- Thay hình hoặc âm lượng chỉ làm lại phần xuất cần thiết. Không chép lời, dịch hay đọc lại giọng khi dữ liệu đó vẫn dùng được.
- Mỗi thay đổi xác định rõ cache nào hết hiệu lực; `finished_result` phải so sánh cả lựa chọn mới có ảnh hưởng tới kết quả. Bản xuất bị thay được cất để mở lại.
- Dừng/lỗi phải dọn file tạm do app tạo, giữ dữ liệu đã hoàn thành và không đụng file người dùng ngoài folder việc.
- Màn chính chỉ giữ lựa chọn thường dùng. Các mục nâng cao nằm trong phần có thể mở/đóng.
- Làm xong từng việc, kiểm tra với dữ liệu thật và ghi nhận giới hạn trước khi sang việc tiếp theo. Không commit khi chưa được yêu cầu.

## 1. Trộn tiếng gốc và tiếng lồng — ưu tiên đầu tiên

**Mục tiêu:** video vẫn giữ được âm thanh nền trong khi lời Việt nghe rõ.

**Phạm vi:**

1. Thêm phần Âm thanh: âm lượng tiếng gốc, âm lượng tiếng lồng, bật/tắt tự hạ tiếng gốc khi có lời Việt.
2. Hạ âm theo khoảng thực tế của giọng đã căn, có chuyển âm mượt ở đầu/cuối; không chỉ dựa vào mốc Whisper ban đầu.
3. Kiểm soát mức âm để tránh trộn quá lớn gây vỡ tiếng. Giữ stereo của nguồn khi có.
4. Có nghe thử đoạn ngắn; lưu lựa chọn theo việc, hàng đợi và bộ cài đặt.
5. Đổi mức âm chỉ trộn/xuất lại. Giữ bản dịch, WAV từng câu và khung hình đã dựng nếu có thể tái sử dụng.

**Mặc định:** tiếng gốc bằng 0 để việc cũ giữ cách xuất hiện tại. Đợt đầu áp dụng cho Video lồng tiếng; Audio và Video phụ đề giữ hành vi đang có.

**Nghiệm thu:** thử nguồn có nhạc nền, khoảng im lặng và nguồn không có audio; tiếng gốc trở lại ở khoảng nghỉ, không chồng giọng bất thường, không mất đoạn đầu/cuối và không tạo lại WAV từng câu khi chỉ đổi âm lượng.

**Lưu ý:** hạ tiếng gốc làm nhỏ cả lời gốc lẫn nhạc đang nằm chung một track. Tách riêng giọng khỏi nhạc là việc khác, chưa nằm trong đợt này.

## 2. Sửa cách đọc tên nước ngoài

**Mục tiêu:** sửa phát âm mà phụ đề vẫn giữ đúng tên người, địa danh và thương hiệu.

**Phạm vi:**

1. Giữ màn Tên hiện có để sửa lỗi chép lời, vẫn áp dụng chung cho phụ đề và giọng.
2. Bổ sung bảng Cách đọc riêng: tên hiển thị → chữ đưa cho giọng đọc, theo ngôn ngữ đích. Có nghe thử bằng giọng đang chọn.
3. Chỉ áp dụng ở đầu vào TTS; không thay lời dịch hay SRT. Không đổi hành vi của bộ chuẩn hóa vendor.
4. Khi sửa bảng, xác định câu bị ảnh hưởng, đọc lại riêng các câu đó và tạo lại phần ghép cần thiết.
5. Lưu được bảng dùng chung theo kênh, kèm bản chụp cấu hình trong từng việc để chạy tiếp nhất quán.

**Nghiệm thu:** tên hiển thị không đổi; đổi cách đọc của một tên chỉ tạo lại các câu có tên đó; kiểm tra tên lồng nhau, khác hoa/thường, tên có dấu và số trong cùng câu. So sánh các WAV không liên quan trước/sau.

## 3. Xem thử sát bản xuất

**Mục tiêu:** người dùng kiểm tra được hình, chữ và âm thanh thực tế trước khi xuất cả video.

**Phạm vi:**

1. Có thao tác Xem thử bản xuất cho đoạn 5–10 giây khi đã có đủ lời/tiếng cần thiết.
2. Dùng cùng cách dựng bằng FFmpeg với bản xuất: khung, vị trí cắt, xuống dòng phụ đề và âm thanh.
3. Giữ Xem trước dịch trực tiếp hiện có; ghi rõ mục đích của hai thao tác trong giao diện.
4. Chọn mốc xem thử, hủy được việc dựng đoạn và dọn cache đoạn thử khi cấu hình đổi.
5. Dùng lại phần dựng chung cho logo và vùng che chữ ở các việc sau.

**Nghiệm thu:** đoạn thử và đoạn tương ứng trong bản xuất có cùng khung, vị trí chữ và mức âm; xem thử không gọi lại dịch/Whisper khi cache đủ; chưa đủ dữ liệu thì báo rõ phần cần làm trước.

## 4. Logo và dấu nhận diện kênh

**Mục tiêu:** đặt dấu nhận diện và dùng lại cho các video tiếp theo.

**Phạm vi:**

1. Chọn ảnh PNG/JPEG trên máy; PNG giữ được độ trong suốt.
2. Chỉnh góc neo, kích thước theo phần trăm khung, lề và độ mờ; nhìn thấy ngay trên khung xem thử.
3. Lưu trong bộ cài đặt. Quản lý bản ảnh dùng cho từng việc để đổi/xóa ảnh nguồn không làm việc cũ âm thầm dùng ảnh khác.
4. Áp dụng sau đổi khung, trước phụ đề Việt; giữ nguyên vị trí tương đối trên video ngang, dọc và vuông.

**Nghiệm thu:** ảnh không méo, không vượt khung; hàng đợi giữ đúng logo lúc thêm; thiếu ảnh thì báo rõ; chỉ đổi logo không dịch hoặc đọc giọng lại.

## 5. Che hoặc làm mờ phụ đề gốc

**Mục tiêu:** giảm sự chồng chéo giữa chữ nguồn và chữ Việt mới.

**Phạm vi:**

1. Khoanh vùng chữ trên hình nguồn bằng chuột; chỉnh vùng bằng số nếu cần chính xác.
2. Chọn làm mờ hoặc che bằng nền màu. Cắt khung vẫn dùng chức năng đang có.
3. Lưu vùng theo tọa độ tương đối của nguồn, xử lý trước bước đổi khung để không bị lệch khi crop.
4. Xem thử vùng đã xử lý cùng khung và phụ đề mới. Đợt đầu áp dụng vùng cố định suốt video.
5. Thứ tự dựng thống nhất: xử lý vùng nguồn → đổi khung → logo → phụ đề Việt.

**Nghiệm thu:** vùng che bám đúng chỗ ở nguồn xoay, dọc và ngang; không vô tình làm mờ chữ Việt; xem thử khớp bản xuất; thay vùng không tạo lại lời và giọng.

**Chưa làm trong đợt này:** OCR tự dò chữ, vùng di chuyển theo thời gian và AI phục hồi cảnh phía sau chữ. Giao diện dùng đúng tên Che/Làm mờ, không quảng cáo là xóa chữ phục hồi nền.

## 6. Hoàn thiện hàng đợi cho nhiều video

**Mục tiêu:** xử lý một nhóm video mà người dùng vẫn kiểm soát được cấu hình và lỗi.

**Phạm vi:**

1. Chọn nhiều video đang chờ để áp bộ cài đặt; xem tóm tắt thay đổi trước khi áp dụng.
2. Sắp xếp lại thứ tự, bỏ qua một việc và thử lại riêng việc lỗi.
3. Thêm lựa chọn Dừng khi lỗi hoặc Tiếp tục video sau; mặc định giữ Dừng khi lỗi như hiện tại.
4. Mỗi việc giữ cấu hình và lỗi riêng, có thời gian xử lý và đường dẫn kết quả.
5. Khôi phục đúng việc đang dở khi mở lại app; không tự chạy khi người dùng chưa bấm.

**Nghiệm thu:** nhóm gồm việc thành công, nguồn hỏng và việc cần sửa tên; đúng chính sách lỗi đã chọn, không mất việc đang chờ, không đổi cấu hình của việc đang chạy. Bước chờ người dùng sửa tên vẫn phải chờ, không tự xác nhận.

## 7. Mở rộng kiểm thử và chuẩn bị bản cài Windows

**Mục tiêu:** có bản cài dùng được trên máy khác, với giới hạn đã được kiểm chứng.

**Phạm vi:**

1. Bộ video ngắn đại diện: Anh/Trung/Nhật/Hàn, nói nhanh, tên riêng, nhạc nền lớn, nguồn dọc/ngang/xoay và nguồn không có audio.
2. Kiểm tra các dịch vụ dịch bằng cấu hình/key người dùng đã cung cấp cho lượt thử; không tự đăng ký hay mua dịch vụ.
3. Thử cả Whisper CLI và đường GPU; giọng native Piper và giọng chạy trong cửa sổ app.
4. Thử YouTube, Douyin, TikTok bằng một video mỗi nguồn; lỗi mạng, link sai và thiếu yt-dlp phải có thông báo phù hợp.
5. Rà lại lưu video, tên file Unicode, đường dẫn có dấu cách, đích không ghi được, hủy lúc xuất và mở bản cũ.
6. Build bản cài bằng `npm run tauri build`, thử trên môi trường Windows sạch, xác định tài nguyên được đóng gói và tài nguyên cần người dùng chuẩn bị. Cập nhật hướng dẫn để phân biệt build frontend với build bản cài.
7. Ghi báo cáo chất lượng có nghe/xem bằng người; không dùng việc Whisper chép lại audio làm bằng chứng duy nhất cho phát âm.

**Nghiệm thu:** bản cài mở được, kiểm tra đồ nghề đúng, hoàn tất một video mẫu và lưu kết quả; không mất dữ liệu khi lỗi/hủy. Mỗi nhánh chưa thử phải được ghi rõ, không đánh dấu hoàn tất theo suy đoán.

## Thứ tự và mốc bàn giao

1. **Mốc A — Âm thanh:** xong việc 1 và 2; bàn giao video so sánh trước/sau.
2. **Mốc B — Nhận diện kênh:** xong việc 3, 4 và 5; xem thử thống nhất với bản xuất, bộ cài đặt giữ đủ lựa chọn mới.
3. **Mốc C — Làm nhiều video:** xong việc 6; chạy một nhóm có cả lỗi và việc cần người dùng xem lại.
4. **Mốc D — Phát hành thử:** xong việc 7; bàn giao bản cài và danh sách giới hạn đã xác nhận.

Mỗi việc được triển khai riêng để dễ xem thay đổi và quay lại. Kiểm thử tương ứng chạy ngay trong từng việc; việc 7 mở rộng phạm vi, không thay thế việc kiểm tra trong các mốc trước.

## Để sau, chưa đưa vào phạm vi triển khai

- Nhân bản giọng hoặc tách giọng theo nhiều người nói: cần người dùng chọn hướng trước.
- Viết lại kịch bản, tự chọn đoạn nổi bật, phụ đề chạy theo từng từ và đăng trực tiếp lên nền tảng: chưa chốt yêu cầu.
- Xóa chữ bằng AI hoặc tách giọng khỏi nhạc: cần đánh giá riêng chất lượng, tài nguyên máy và cách chuẩn bị model; không tự tải thêm.
- Đổi tên app hoặc ngôn ngữ giao diện: cần hỏi trước theo AGENTS.md.

Không ấn định ngày hoàn thành khi chưa đo phần trộn âm, dựng đoạn thử và đóng gói trên máy đích. Chốt phạm vi từng việc, làm và kiểm chứng theo thứ tự trên.
