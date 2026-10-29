# Thử tốc độ với video Douyin — 29/10/2026

## Nguồn và cấu hình

- Link người dùng đưa: https://v.douyin.com/IrX0XVPXjUY/
- Video `7688923388256980274`: hướng dẫn workflow ComfyUI/MiniMax H3, dài **363,233 giây**, H.264/AAC, **1920×1080**, **77.274.244 byte**.
- SHA-256 nguồn: `4C516812C51BB4D5EDCF34F2DEC32253A5136DDDE9D1AE4170F22263FA7B9FB8`.
- Tải bằng đúng `src-tauri/douyin/fetch.py` của app: phiên khách Edge và bộ ký request hiện có. Không dùng yt-dlp cho Douyin.
- Chạy cửa sổ Tauri dev thật; gọi `start_dub` qua IPC và giữ bridge/worker TTS thật của app. Dịch DeepSeek local `deepseek-chat`, giọng đã chọn `ngochuyennew`, tiếng Việt, bật thuật ngữ, xuất video khung gốc có chữ, cấu hình render mặc định.
- Hai lượt dùng engine giọng trong cửa sổ. Có điểm gắn đo tạm để ép cùng engine và chọn tuần tự/cuốn chiếu; đã gỡ khỏi mã sản phẩm. Giữa hai lượt nạp lại trang để worker giọng bắt đầu lại. Không đổi lựa chọn dịch, giọng hoặc prefs của người dùng.
- Server DeepSeek ban đầu tắt, được bật để thử rồi tắt lại. Cửa sổ dev có điểm gắn đo cũng được đóng sau khi xong. Không commit.

## Cách so sánh

Lượt tuần tự chạy nguồn mới từ đầu. Whisper GPU mất **38,0 giây**, app ngắt thành **102 câu**, AI không đề xuất sửa tên; danh sách tên rỗng được duyệt, thuật ngữ bật như cấu hình đang lưu.

Một lượt chép lời độc lập tiếp theo mất 34,0 giây nhưng cho 100 câu. Đã dừng lượt đó ở bước tên, trước dịch/TTS, và giữ dữ liệu riêng ở `output/kiem-thu-20261029/overlap-asr-check`. Không dùng nó làm số đo A/B.

Để giữ đầu vào giống hệt, lượt cuốn chiếu chính dùng bản sao `transcript.json`, `lines.json`, `names.json` và `terms.json` từ lượt tuần tự; SHA-256 transcript hai lượt khớp. **Không sao chép translation hoặc WAV**: lượt cuốn chiếu vẫn gọi dịch và đọc giọng mới hoàn toàn. Do đó bảng so sánh chỉ lấy từ khi bắt đầu bước dịch, không lấy hai tổng pipeline có/không có Whisper để so trực tiếp.

Các số đo dưới đây không tính tải mạng, Whisper, duyệt tên và thuật ngữ. Mốc “đến kết quả” lấy tới lúc IPC trả file hoàn tất, gồm cả phần hoàn tất lịch sử sau log pipeline. “WAV đầu tiên” lấy theo sự kiện tiến độ bridge (có sai số khoảng một nhịp báo tiến độ), không phải tiếng đã phát tới tai người dùng.

## Kết quả

| Phần đo | Tuần tự | Cuốn chiếu |
| --- | ---: | ---: |
| Từ bắt đầu dịch đến IPC trả kết quả | **525,93 giây — 8:46** | **381,21 giây — 6:21** |
| Từ bắt đầu dịch đến báo WAV đầu tiên | **30,08 giây** | **18,07 giây** |
| Dịch ban đầu | 19,54 giây | 17,43 giây |
| Dịch + TTS đầu tiên chạy chồng nhau | không | TTS bắt đầu ở 10,13 giây, dịch xong ở 18,13 giây tính theo lượt |
| Dịch + toàn bộ đọc/rút gọn/đọc lại, theo timing.log | 328,9 giây | 202,6 giây |
| Căn tiếng | 5,3 giây | 4,8 giây |
| Ghép video và ghi chữ | 173,0 giây | 154,8 giây |

Lượt cuốn chiếu về kết quả sớm hơn **144,71 giây, khoảng 27,5%** trong phép đo từ bước dịch. Đây là số quan sát của **một cặp lượt**, không phải kết luận cơ chế cuốn chiếu tự tạo mức tăng tốc 27,5%.

**Yếu tố gây nhiễu lớn:** dù cùng lời gốc và glossary, dịch vụ trả câu chữ khác nhau và quyết định rút gọn khác nhau. Tuần tự đọc lại 47 + 17 câu, cuốn chiếu đọc lại 21 + 14 câu. Tổng WAV cuối cũng khác: 307,66 giây và 298,11 giây. Ghép video chênh thêm 18,2 giây. Khoảng dịch thực sự được che bởi TTS trong lượt cuốn chiếu chỉ khoảng **8 giây**. Chưa có phép đo đảo thứ tự/lặp nhiều lượt hoặc phát lại chính xác cùng response model để tách riêng tác động của bộ lập lịch.

Tổng active trong `timing.log` của lượt tuần tự là 559,2 giây (9:19), chưa tính chờ duyệt tên và bước hoàn tất sau pipeline. Tổng 362,4 giây của lượt cuốn chiếu bỏ qua phần chuẩn bị đã cache, nên không dùng hai số này để công bố chênh lệch end-to-end từ video mới.

## Chất lượng và kiểm tra file

- Cả hai bản có **102 câu dịch không trống, 102 WAV từng câu**. FFprobe đọc được video 1920×1080, H.264/AAC, dài tương ứng nguồn; FFmpeg giải mã hết cả hai bản không lỗi.
- Không còn `audio.wav`, `dub.wav`, `*.fit.wav` hoặc `*.partial.*` trong hai folder đã hoàn tất.
- Đã xem ảnh nguồn ở nhiều mốc và ảnh bản cuốn chiếu ở giây 15: giữ khung nguồn, có chữ Việt, chữ Trung vốn có trong nguồn vẫn còn.
- Lời Whisper có **11 lần** lặp `字幕志愿者 杨茜茜` ở đoạn minh họa, cùng một số đoạn có dấu hiệu nhận dạng sai. Không dùng việc có đủ WAV để kết luận nội dung đúng.
- Báo cáo căn tiếng: **49 câu bị cắt** ở bản tuần tự, **41 câu bị cắt** ở bản cuốn chiếu (so `kept < spoken - 0.01`), tổng phần âm thanh sau tăng tốc không được giữ khoảng 28,54 và 28,59 giây. Cả hai còn vấn đề chất lượng thực tế; chưa nghiệm thu bằng người nghe trọn video.

Nút thắt quan sát được là đọc giọng nhiều lần và ghi chữ Full HD; dịch ban đầu chỉ khoảng 18–20 giây. Việc đáng khảo sát tiếp là hạn chế lời chép lặp/nhận nhầm và gom câu vụn, dịch/rút gọn theo thời lượng trước khi đọc, rồi đo lại số lượt TTS và mức cắt. Không thay các hành vi đó trong lần benchmark này.

## Dữ liệu kiểm chứng

- Script điều khiển, sự kiện, đối chiếu WAV/FFprobe/giải mã và ảnh: `.bench-asr/douyin-speed-20261029/`.
- Bảng chi tiết: `.bench-asr/douyin-speed-20261029/audit.json`.
- Bản tuần tự: `output/kiem-thu-20261029/sequential/douyin-comfyui/douyin-comfyui.vi.chu.mp4`.
- Bản cuốn chiếu: `output/kiem-thu-20261029/overlap/douyin-comfyui/douyin-comfyui.vi.chu.mp4`.

Sau khi gỡ điểm đo tạm, `cargo test --manifest-path src-tauri/Cargo.toml --offline --lib --quiet`: **284 passed, 0 failed, 3 ignored**. `git diff --check` sạch; `piper.rs` không còn thay đổi so với Git và mã pipeline khớp bản trước khi gắn benchmark.
