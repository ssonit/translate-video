# Chế độ Chỉ hiện chữ

Ngày: 2026-10-22

## Mục tiêu

Một chế độ mới, cạnh **Chỉ lấy audio**. Video kết quả hiện chữ đã dịch trên hình, tiếng gốc giữ nguyên, không đọc giọng mới. Thiếu giọng Piper không chặn chế độ này.

## Đã chốt

Đây là lựa chọn của người viết plan, vì người dùng bảo làm một chế độ rồi lập plan, không chọn từng nhánh.

- Checkbox **Chỉ hiện chữ** trên màn soạn và màn việc dở. Bật cái này thì tắt **Chỉ lấy audio**, và ngược lại. **Ghi chữ vào video** bị khóa: chế độ này luôn đốt chữ.
- Chạy đủ tách tiếng, chép lời, ngắt câu, tên, thuật ngữ, dịch. Bỏ đọc giọng và co độ dài. Lần đầu vẫn dừng ở bước Tên.
- Ghép bằng cách đốt phụ đề lên video gốc và copy tiếng gốc. Không tạo `dub.wav`, không tạo wav từng câu, không tạo bản mp4 lồng tiếng không chữ.
- File kết quả là `{tên}.{mã}.chu.mp4`, kèm `{tên}.{mã}.srt`. Thư viện và «đã có bản» nhận file này khi folder không có bản mp4 lồng tiếng.
- Đổi sang hoặc rời chế độ này trên cùng folder thì xóa video kết quả, bản có chữ, `dub.wav`, file `.m4a` đã xuất và `fit-report.json`. Bản dịch giữ nếu ngôn ngữ, thuật ngữ và «không dịch» không đổi. Wav từng câu giữ nếu chuỗi giọng trong `settings.json` không đổi.
- Xem trước phát video gốc, tiếng gốc không hạ, chữ hiện khi câu đã dịch. Không gọi giọng. Không dừng ở bước Tên. `translation.json` chỉ ghi khi mọi câu xong.
- Sửa câu trên bản chỉ chữ thì ghi lại phụ đề và đốt lại. Không hỏi giọng.
- Hàng đợi cũ không có cờ này thì hiểu là tắt. Không ghi cờ vào `prefs.json`.
- Link vẫn tải video, không đi đường chỉ lấy phụ đề.

## Không làm

- Không phụ đề mềm trong file (không gắn track subtitle). Có file `.srt` cạnh video.
- Không đổi kiểu chữ đang đốt (Arial 18, mép đen, đáy hình).
- Không đổi lồng tiếng có giọng, không đổi **Chỉ lấy audio**, không đổi **Ghi chữ vào video** khi chế độ này tắt.
- Không thêm nút tải. Không gửi audio hay video đi. Không đổi tên app.
