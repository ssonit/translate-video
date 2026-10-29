# Tối ưu lời chép và giọng — 29/10/2026

## Thay đổi

1. Faster-whisper bật VAD bằng Silero đi kèm thư viện đang cài, khoảng im lặng 500 ms, đệm giọng 200 ms. Tắt `condition_on_previous_text` để tránh một đoạn nhận nhầm kéo thành chuỗi lặp về sau. Không tải model/thư viện mới, không xóa câu theo một danh sách chữ bị nghi ngờ. File transcript và dòng tiến độ giữ định dạng cũ.
2. Khi tạo `lines.json` mới, ghép các mẩu chưa kết thúc câu sát nhau: khoảng hở tối đa 0,45 giây, nhóm tối đa 8 giây và 75 ký tự. Không ghép qua dấu kết câu, khoảng nghỉ dài, mốc chồng nhau rõ hoặc nhóm quá dài. Giữ từ và mốc đầu/cuối; sau đó vẫn chia câu dài theo đường hiện có. Việc đã có `lines.json` và các bản biên tập không bị tự chia lại.
3. Bản dịch để lồng video được cung cấp thời lượng từng câu ngay trong request ban đầu và các request dịch lại. Cửa sổ tính cả khoảng trống được phép mượn theo `timeline::room`; tiếng Việt có gợi ý 3,2 âm tiết/giây. Đây là mục tiêu mềm: yêu cầu giữ ý, tên, số liệu và phủ định, không bỏ/cắt câu để đạt số đếm. Audio liền mạch, chế độ chỉ chữ, Xem trước và Nghe trước giữ đường dịch không ép ngân sách này.
4. Chỉ xin rút gọn nếu câu không vừa ngay cả ở mức tăng tốc nhẹ hiện có (`MAX_SPEED = 1.08`). Không tăng giới hạn tốc độ. Audio liền mạch không còn bị xin rút gọn theo khoảng hình dù không cần bám mốc. Bản rút gọn sai ngôn ngữ bị từ chối.
5. Bridge đọc giọng có thử lại một lần cho câu ngắn đứng quá 45 giây: dừng worker cũ, nạp lại cùng giọng và đọc lại đúng câu đó. Không khởi động lại việc đã hủy hoặc lỗi model thông thường; đoạn dài giữ timeout hiện có. Các WAV trước đó không phải tạo lại.

## Thử lại video Douyin

Dùng cùng file `douyin-comfyui.mp4` dài 363,233 giây từ https://v.douyin.com/IrX0XVPXjUY/, DeepSeek local `deepseek-chat`, giọng `ngochuyennew`, thuật ngữ bật, khung gốc Full HD có chữ. Giữ cùng engine trong cửa sổ để so với [lượt trước](thu-toc-do-douyin-2026-10-29.md). Điểm ép engine chỉ phục vụ thử nghiệm, không phải thiết lập mới của sản phẩm.

Whisper chạy mới mất **25,2 giây**, trước đó 38,0 giây. Không còn 11 câu lặp `字幕志愿者 杨茜茜`. Mốc lời thuyết minh đầu và cuối còn trong transcript; chưa có người nghe nghiệm thu toàn bộ phần minh họa có nhạc/nói lẫn nhau. VAD có thể bỏ lời quá nhỏ, lời hát hoặc tiếng bị nhạc che; không suy ra độ chính xác nhận dạng chỉ từ số câu giảm.

Sau ghép mẩu, có **45 câu**, thay vì 102. Từ bộ lời mới, chạy dịch và tạo WAV mới hoàn toàn, không sao chép bản dịch/tiếng cũ. Phép so dưới đây tính từ lúc bắt đầu dịch, nên không cộng/trừ phần chuẩn bị đã cache.

| Phần đo | Cuốn chiếu trước tối ưu | Sau tối ưu |
| --- | ---: | ---: |
| Từ bắt đầu dịch tới IPC trả kết quả | 381,21 s — 6:21 | **323,98 s — 5:24** |
| Dịch và toàn bộ xử lý giọng, theo log | 202,6 s | **122,2 s** |
| Câu đưa vào TTS lần đầu | 102 | **45** |
| Câu thực sự đọc lại sau rút gọn | 21 + 14 | **0** |
| Câu bị cắt theo fit-report | 41 | **4** |
| Tổng phần không giữ sau tăng tốc, theo fit-report | 28,59 s | **4,54 s** |
| Ghép và ghi chữ Full HD | 154,8 s | 175,7 s |

Lượt mới nhanh hơn khoảng **57 giây (15%)** tính từ bước dịch; riêng phần dịch/giọng giảm khoảng 40%. Ghép Full HD vẫn là phần lớn thời gian còn lại. Đây là một cặp lượt với đầu vào được cải thiện và response model không cố định; không phải cam kết tăng tốc cho mọi video.

Lượt mới vẫn hỏi rút gọn 5 câu một lần nhưng không nhận bản ngắn hơn phù hợp để đọc lại. Không xóa/sửa tay bốn câu còn bị cắt để làm đẹp số liệu.

## Sự cố quan sát và dữ liệu

Lượt thử đầu có timeout bộ đọc sau 20/45 câu. Câu bị nghi ngờ đọc riêng vẫn chạy bình thường; chưa xác định nguyên nhân. Đã giữ log và 20 WAV ở `output/kiem-thu-20261029/optimized-failed`, không dùng thời gian lỗi làm benchmark. Lượt thử mới bắt đầu từ cùng bộ transcript/lines/names/terms, dịch/TTS mới trong folder riêng, hoàn tất 45/45 WAV.

Bản xuất: `output/kiem-thu-20261029/optimized/douyin-comfyui/douyin-comfyui.vi.chu.mp4`. FFprobe đọc được H.264/AAC, 1920×1080; FFmpeg giải mã toàn bộ không lỗi, 45 bản dịch không trống, 45 WAV. Không còn audio.wav, dub.wav, WAV đã fit hoặc video partial. Đã xem ảnh kết quả ở giây 15; chữ Trung có sẵn trong nguồn vẫn còn.

Script, log, ảnh, audit và bảng so: `.bench-asr/optimize-20261029/`. Không thay các bản xuất cũ.

## Kiểm tra

- `python -m unittest discover -s scripts -p faster_whisper_test.py`: 11 test đạt.
- Test Rust bao gồm giữ nội dung/mốc khi ghép mẩu, không ghép qua ranh giới câu/nghỉ/chồng mốc/giới hạn, ngân sách khớp ID trong lô và không áp dụng sang chế độ nghe.
- Test phục hồi kiểm tra đọc lại đúng một lần, giữ câu sau, không thử lại lỗi model/công việc hủy, và dừng khi lần hai vẫn bị đứng.
- Sau khi nạp lại trang để dùng bridge mới, giả lập lần đọc đầu không trả kết quả trong cửa sổ thật: app tự thay worker và trả WAV 254.508 byte sau 56,9 giây, không cần chạy lại video. Lượt giả lập trước khi nạp lại trang bị hủy vì còn handler cũ trong phiên dev; không dùng nó để kết luận hành vi bản mới.
- `npm run verify:local`: frontend, queue, playback, windows-package, build, Rust đều PASS; báo cáo `.bench-asr/verify-1793245019224/report.json`.
- Sau khi gỡ điểm đo ép engine: Rust **287 passed, 0 failed, 3 ignored**; không còn thay đổi ở `piper.rs`. `git diff --check` không lỗi khoảng trắng.

Việc cũ giữ cache hiện có; muốn lấy lời chép/gom câu theo chính sách mới thì dùng Chép lời lại hoặc tạo việc mới. Không chuyển giọng/dịch vụ, không tải thêm tài nguyên và không gửi audio/video lên mạng.
