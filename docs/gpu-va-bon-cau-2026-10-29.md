# NVENC và bốn câu bị cắt — 29/10/2026

## Đường xuất video

Các lần phải mã hóa hình (ghi phụ đề, đổi khung, xuất đoạn thử) thử `h264_nvenc`, preset `p5`, tune `hq`, VBR CQ 25, `-b:v 0`, H.264 8-bit `yuv420p`. Bộ lọc khung/chữ vẫn chạy CPU. Ghép tiếng khi không đổi hình vẫn copy video, không mã hóa lại.

Nếu FFmpeg/NVENC lỗi, chạy lại bằng đúng lệnh CPU trước đây (`libx264`, `veryfast`, CRF 20), ghi đè file partial của lượt xuất. Hủy thì dừng, không thử CPU. Hai đường dùng chung kiểm tra kết quả, đổi tên và dọn partial hiện có. Lỗi GPU được ghi ở stderr của app. Không cài driver hay tải thư viện, không thêm lựa chọn model/giao diện.

Thông số VBR/CQ theo [tài liệu NVIDIA về FFmpeg](https://docs.nvidia.com/video-technologies/video-codec-sdk/13.1/ffmpeg-with-nvidia-gpu/index.html); CQ 25 là lựa chọn sau phép đo bên dưới, không coi số CQ bằng số CRF.

## Đối chiếu trên cùng video

RTX 4060 Laptop, driver 591.86. Video Douyin dài 363,233 giây, 1920×1080; dùng bản chưa có chữ của lượt `optimized`, cùng âm thanh và cùng ASS do bộ dàn chữ sản phẩm tạo. Các lượt chạy lần lượt, không đổi độ phân giải, tốc độ khung hay phụ đề.

| Encoder | Thời gian ghi chữ | Dung lượng (MB thập phân) | SSIM toàn khung |
| --- | ---: | ---: | ---: |
| CPU veryfast CRF 20 | 128,84 s | 155,65 | 0,994626 |
| NVENC P5 CQ 19 | 29,97 s | 276,96 | 0,997390 |
| NVENC P5 CQ 23 | 29,86 s | 215,04 | 0,996486 |
| **NVENC P5 CQ 25 (chọn)** | **29,70 s** | **177,51** | **0,995555** |

CQ 25 giảm thời gian phần ghi chữ khoảng **77% (4,34 lần)**, file lớn hơn CPU **14%**. Bản CPU đo lại có SHA-256 trùng bản có chữ của lượt optimized, xác nhận đầu vào/bố cục tương ứng. FFprobe xác nhận bản GPU có tag encoder `h264_nvenc`, không phải CPU dự phòng.

SSIM so từng bản giải mã với **nguồn chưa có chữ + đúng bộ lọc ASS**, trên toàn bộ khung video. Không dùng bản CPU đã nén làm hình tham chiếu. Đã xem ảnh CPU/GPU ở giây 15: chữ Việt và chi tiết UI giữ được, chưa thấy lỗi rõ rệt. SSIM và một ảnh không thay thế đánh giá cảm nhận trên mọi cảnh. Đây là một lượt mỗi cấu hình, không phải cam kết tốc độ/chất lượng cho mọi video. Mốc 175,7 giây trong báo cáo trước gồm ghép và ghi chữ ở lượt khác; không dùng nó làm mẫu CPU của bảng này.

## Sửa từng câu trên bản sao

Giữ nguyên việc ở `output/kiem-thu-20261029/optimized/`. Việc mới ở `output/kiem-thu-20261029/repaired-gpu/douyin-comfyui/`. Không tăng `MAX_SPEED = 1.08`, không tự áp chính sách sửa mốc này sang video khác.

| Câu | Phần bị bỏ trước đây, sau tăng tốc | Cách sửa | Kết quả |
| --- | ---: | --- | --- |
| 1 | 2,385 s | Rút lời mở đầu; dời ranh giới câu 1/2 từ 4,44 sang 4,54 s | Giữ đủ WAV mới, tốc độ 1,0704× |
| 17 | 1,242 s | Giữ nguyên lời/WAV; dời ranh giới 16/17 từ 104,24 về 102,84 s, dùng khoảng cuối câu 16 còn dư | Giữ đủ WAV, tốc độ 1,0577× |
| 44 | 0,031 s | Giữ nguyên lời/WAV; dời ranh giới 43/44 từ 347,47 về 347,37 s | Giữ đủ WAV, tốc độ 1,0704× |
| 45 | 0,879 s | Rút phần kết, giữ lời kêu gọi thích/theo dõi/bình luận 666, cảm ơn và chào | WAV 7,860 s, tốc độ tự nhiên |

Lời mới câu 1: “Điền prompt một chạm, tạo video 1080p không giới hạn thời lượng.” Bỏ lời chào và cách dẫn “hôm nay tôi chia sẻ”, giữ các thông tin thao tác, độ phân giải và thời lượng.

Lời mới câu 45: “Một trợ thủ rất tốt. Thấy hữu ích, nhớ like, theo dõi, bình luận 666. Cảm ơn, đồ ở chỗ cũ, tạm biệt.”

Chỉ hai câu 1/45 đọc lại bằng `TtsEngine`/Web Worker thật, giọng `ngochuyennew`, bộ chuẩn hóa hiện có. **43 WAV còn lại trùng SHA-256** bản trước. Không gọi dịch, không gửi audio/video lên mạng; không bật DeepSeek. Lời Whisper/`lines.json` giữ nguyên, lời rút gọn lưu trong `translation.json`, mốc sửa lưu đúng định dạng `timing.json` và qua kiểm tra revision/chồng mốc của lệnh `revise_lines`.

Xuất lại qua IPC `revise_lines` thật mất **80,85 giây**, gồm chuẩn bị/lịch sử, căn tiếng, ghép, ghi chữ và hoàn tất; hai WAV sửa đã tạo trước, không tính vào số này. Không coi đây là phép đo pipeline mới từ Whisper. Căn tiếng chuyển từ **4 câu bị cắt / 4,54 giây bị bỏ** thành **0 câu bị cắt**; 45 bản dịch không trống và đủ 45 WAV. Câu 17/44 bắt đầu sớm hơn lời gốc như bảng; chưa có người nghe nghiệm thu cả video và độ khớp thao tác minh họa.

Bản xem: `output/kiem-thu-20261029/repaired-gpu/douyin-comfyui/douyin-comfyui.vi.chu.mp4`. FFprobe: H.264 NVENC/AAC, Full HD, thời lượng khớp nguồn; giải mã toàn bộ không lỗi. Không còn audio.wav, dub.wav, WAV fit hay video partial trong việc đang dùng; bản cũ nằm riêng.

## Kiểm chứng và dữ liệu

- Rust: **291 passed, 0 failed, 4 ignored**. Bốn test mới kiểm tra GPU lỗi → CPU giữ nguyên tham số, thành công không thử lại, hủy không thử lại, remux giữ nguyên và lỗi CPU trả về; có thử FFmpeg thật với GPU index không tồn tại, ghi đè partial rồi giải mã bản CPU thành công.
- `npm run build` đạt. `git diff --check` đạt.
- Test đo nguồn local `benchmark_local_video_encoders` được ignore mặc định; chạy với ba biến `VIDEO_BENCH_INPUT`, `VIDEO_BENCH_SRT`, `VIDEO_BENCH_OUTPUT` trỏ đường tuyệt đối, rồi `cargo test --manifest-path src-tauri/Cargo.toml --offline --lib benchmark_local_video_encoders -- --ignored --nocapture`. Đường output phải là folder benchmark riêng; không đặt vào folder việc gốc.
- Dữ liệu local: `.bench-asr/gpu-20261029/encode/quality.json`, log SSIM, ảnh `cpu15.png`/`nvenc25-15.png`, `voices.json`, `edits.json`, `repair-events.jsonl`, `repair-result.json`, `audit.json` và script đo/đối chiếu. Lượt `encode-test.log` đầu dùng CQ 19; CQ 23/25 đo bằng `quality.py` với cùng tham số còn lại. Mã sản phẩm cuối dùng CQ 25.

Chưa làm mục đánh dấu lời chép đáng ngờ trong phạm vi lần này. Không commit.
