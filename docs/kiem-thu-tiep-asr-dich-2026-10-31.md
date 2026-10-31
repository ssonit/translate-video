# Kiểm thử bổ sung ASR, dịch và xuất — 31/10/2026

## Phạm vi và môi trường

Repo bắt đầu sạch ở `52cb883`. Không sửa mã sản phẩm, không commit/push. Bằng chứng mới trong `.bench-asr/continue-20261031/`; không coi các báo cáo trước là lượt thử mới.

Dùng EXE release có SHA-256 `58F7D8A89DF820F574963AB2392C7DBCB2542ECF9316B171F32918A5FDD854DF`, giống EXE bàn giao ở đợt trước. Ba tiến trình EXE riêng, profile WebView riêng, CDP loopback 9223–9225. Runtime được cung cấp đồ nghề đã có bằng junction trong thư mục thử (voices, whisper, piper, scripts và Deepseek-API); không cài/tải thêm, không đổi đường tìm đồ nghề của sản phẩm. Gói ZIP bàn giao không tự nhiên có thêm các tài nguyên này.

Người dùng trực tiếp chọn **Bật DeepSeek local để thử**. Bật bằng lệnh app `start_deepseek_server`, dịch `deepseek-chat` qua server 127.0.0.1:8000; chỉ chữ lời thoại đi qua dịch vụ. Không đổi lựa chọn dịch/giọng/prefs toàn app. Giọng nháp `ngochuyennew`, thuật ngữ tắt. Tắt server khi xong, xác nhận không còn listener 8000/9223/9224/9225 và dừng đúng ba PID/path thử. Không dừng app/dịch vụ khác của người dùng.

## Chép lời mới toàn nguồn dài

Nguồn thật: `output/tai-ve/Build & Sell Grok Bots (2 Hour Course) [4hKJ9X6rGFo].mp4`, 1.806.307.428 byte, 6.341,094 giây. Gọi `open_draft` vào folder kết quả hoàn toàn mới `full-asr/…`; không chép transcript, phụ đề, bản dịch hoặc WAV cũ.

- Mở nguồn: **1.571,1 ms**.
- `prepare_draft`: **431.353 ms — 7 phút 11,353 giây**, gồm tách tiếng, ASR, chia câu và trả trạng thái; không phải đồng hồ riêng phần tính Whisper.
- 2.159 đoạn trong transcript, 2.770 câu sau chia/gom; không mốc âm hoặc `end <= start`.
- Có lời từ giây 0 tới 6.340,86; đây là kiểm tra phạm vi mốc, không chứng minh không bỏ lời.
- Không có metadata dùng phụ đề nguồn; `audio.wav` đã dọn sau thành công.
- Mẫu mới giữ `$5 million of top line revenue` trong cùng cụm nguồn ở giây 51,72–53,80, tránh ranh giới `$5` / `million` của cache cũ.

Bằng chứng: `asr-start.json`, `asr-progress.json`, `asr-audit.json`, driver `full-asr.js`; cache ở `full-asr/…`. Đây là một lượt trên máy đang có các tác vụ khác, file hệ điều hành ấm; không phải benchmark cô lập/cold disk và không phải chép lại toàn nguồn bằng CPU.

## Đường Whisper CLI trong app

Tạo runtime thử riêng `cpu-app/` chỉ có whisper-cli/ggml-small, không cung cấp thư mục large-v3-turbo. Không thay model hoặc DLL của máy. Gọi `open_draft` rồi `prepare_draft` trên đoạn nguồn thật 60 giây, chưa có transcript/cache.

- App tự chọn `whisper-cli.exe`, lệnh thật được chụp trong `cpu-process.json`, chạy `-t 2 -l auto -oj -pp`; thư mục binary đang dùng chỉ có backend CPU, không có DLL CUDA.
- Hoàn tất **47.247 ms**, 31 câu, thời lượng nguồn 60 giây.
- Đây là kiểm tra app chọn đường CLI khi thiếu điều kiện faster-whisper; **không** giả lập GPU hỏng sau khi model CUDA đã chạy, không khẳng định đã có cơ chế tự retry lỗi CUDA bằng CPU.

Sau đó đổi riêng cấu hình nháp sang dịch Việt và giọng đọc để kiểm tra xuất. `editing.json` là lời hiệu lực; canonical `translation.json` vẫn có thể giữ lời nền cũ. Không dùng canonical để kết luận nhầm rằng video không được dịch.

## Dịch mới, giọng và xuất

Đoạn `clips/minute.mp4` được dựng lại chính xác từ giây 40 tới 100 của nguồn bằng FFmpeg (H.264/AAC), không dùng stream copy lệch keyframe. Ba nháp độc lập cùng dùng đoạn này:

| Nháp / nguồn lời | Chuẩn bị vùng | Xuất từ nháp thiếu |
| --- | --- | --- |
| `fresh-minute`: transcript cũ, chia lại 35 câu | Chữ 0–30s: 11.800,6 ms, 16 câu có lời, **0 WAV**. Nghe 0–12s: 12.357,7 ms, **9 WAV** | **28.551,7 ms**, 35 WAV, MP4 không đốt chữ |
| `cpu-output/minute`: ASR CLI mới, 31 câu | Nghe 0–30s: 36.628,5 ms, **18 WAV** | **33.062,2 ms**, đủ 31 WAV, MP4 không đốt chữ |
| `gpu-minute`: transcript từ lượt ASR toàn nguồn mới, 34 câu | Chữ 0–30s: **7.016 ms**, 18 câu có lời, **0 WAV**. Nghe 0–12s: **17.560,8 ms**, **10 WAV** | **46.906 ms**, đủ 34 WAV, MP4 có đốt chữ |

Các số là thời gian từng thao tác, không cộng thành thời gian end-to-end vì có khoảng dừng thao tác/driver. Có các phiên thử chạy đồng thời; không dùng bảng làm so sánh nhân quả giữa ASR hoặc engine.

Xuất thực sự đi qua sự kiện `names-review`; driver ghi đề xuất rồi tự xác nhận trên fixture. Hai nháp đầu đề xuất Grockbot(s) → Grokbot(s), nháp từ ASR toàn nguồn không đề xuất sửa. Đây là nghiệm thu luồng kỹ thuật, không phải duyệt tên bằng người. Sau duyệt tên/lời chính thức, lời nháp có thể được dịch lại và WAV phải tạo lại; không hứa giữ mọi WAV nháp trước xuất. Mẫu nháp đầu có 9 WAV trước xuất và cả 9 đã thay đổi ở lần xuất chính thức.

Trong bản dịch mới, cụm tiền thành “doanh thu gần 5 triệu đô”, không còn lỗi “5 đô la / triệu doanh thu hàng đầu” của cache trước. Chưa đọc đối chiếu toàn bộ bản dịch hoặc nghe nghiệm thu bằng người; các đoạn khó khác vẫn phải đánh giá riêng.

## Preview, sửa câu và xác minh media

- WebView production thật mở editor `fresh-minute`, chọn Giọng lồng và Phát. 140 mẫu mỗi 500ms cho thấy chạy tới **59,98433/60 giây**, không alert và không nhảy lùi. Có screenshot `ui-playback.png` đã xem lại. Đây là DOM/media thật qua CDP, không phải lượt click Windows bản địa, và không phải phát liên tục toàn video 105 phút.
- Sửa riêng câu cuối của nháp 35 câu bằng cách thêm lời thử rồi xuất lại: sau lưu còn **34 ready**, hoàn tất lưu + xuất **9.794,6 ms**. **34 WAV giữ nguyên SHA-256 và mtime; chỉ 035.wav thay đổi**. Lời thêm chỉ phục vụ kiểm tra cache, không dùng bản này đánh giá dịch đủ ý.
- Ba MP4 mới đều giải mã toàn file bằng FFmpeg `-xerror` không lỗi; không còn `.partial`, `dub.wav` hay `.fit.wav` ở ba việc hoàn tất.
- Nháp ASR toàn nguồn → mẫu 60s: 34 clip preview sau xuất khớp `voice-spans.json`; sai khác đầu/cuối lớn nhất **0 giây**. Lịch khớp không có nghĩa giọng không bị cắt.
- SHA-256, kích thước và mtime nguồn gốc giữ nguyên. Không dùng folder việc thật làm đích thử.

Bằng chứng: `fresh-result.json`, `fresh-audit.json`, `cpu-export-result.json`, `cpu-effective-audit.json`, `gpu-export-result.json`, `gpu-post-export-region.json`, `ui-playback.json/png`, `repeat-result.json`, `repeat-audit.json`, `final-audit.json`.

## Chất lượng chưa đạt: giọng bị cắt

Đếm đúng hợp đồng `FitEntry::is_cut`: `spoken - kept > 0,02s`; `spoken` trong report đã tính tốc độ. Đây là phần giọng không được giữ, không phải tổng độ dài WAV gốc.

| Nháp, bản hiện được giữ trong fixture | Câu bị cắt | Tổng giọng không giữ |
| --- | ---: | ---: |
| Mốc chép cũ, sau thử sửa câu cuối | **14/35** | **7,66 giây** |
| ASR CLI mới | **15/31** | **5,11 giây** |
| ASR toàn nguồn mới | **19/34** | **11,11 giây** |

Bản mốc cũ sau thử sửa có thêm lời kiểm tra ở câu cuối; không dùng nó làm A/B chất lượng với các bản còn lại. Snapshot đầy đủ của ba báo cáo hiện tại: `fit-snapshots.json`.

Đoạn thử bắt đầu/kết thúc giữa lời, có thể làm hai câu biên quá ngắn. Nhưng mẫu cuối còn **17 câu bị cắt bên trong**, tổng khoảng **7,45 giây**, nên không thể quy toàn bộ vấn đề cho ranh giới cắt video. Chưa có phép so giữ cố định response dịch và cấu trúc câu, không kết luận engine ASR nào gây nhiều lỗi hơn từ ba lượt này.

Không tự sửa mốc, bỏ ý, tăng MAX_SPEED hoặc cắt bớt dữ liệu để làm đẹp kết quả. Chưa xác nhận giọng tự nhiên, phát âm tên/số đúng hoặc giữ đủ thông tin bằng nghe người. **Không nghiệm thu chất lượng lồng tiếng chỉ vì file giải mã được và đủ WAV.**

## Tắt dịch vụ và xử lý lỗi

Sau các lượt dương tính, gọi `stop_deepseek_server`, rồi yêu cầu dịch vùng mới của nháp dài:

- Báo ngay “Chưa bật DeepSeek local…”, không tự bật server.
- Backend trả `running:false`, server không chạy.
- Tất cả JSON của nháp dài giữ nguyên hash/mtime so với trước thử lỗi.

Bằng chứng: `server-off-test.json`, `server-off-audit.json`, `stopped-processes.json`. Các lượt driver sai ban đầu (chưa tạo output dir, runtime release chưa được cung cấp đồ nghề, lỗi encoding stdout của script audit Python) đã sửa trong harness; không tính chúng là lỗi sản phẩm. Driver/audit chỉ ở `.bench-asr`, không có hook mới trong `src`/Rust.

## Kết luận và việc tiếp theo

Đợt này bổ sung được ASR toàn nguồn thật, đường CLI qua app, dịch DeepSeek mới, giọng/xuất thật từ nháp thiếu, phát trọn đoạn, cache sửa một câu, lịch preview/export và lỗi khi dịch vụ tắt. Không sửa mã nên không chạy lặp verify:local chỉ để thay số đếm; bộ hồi quy đã có ở `.bench-asr/verify-1793435276830/` là bằng chứng trước đó, không ghi thành lượt mới.

Ưu tiên tiếp theo là **chất lượng thời lượng câu**, trước khi thêm API/model hay tiếp tục tối ưu tốc độ: cảnh báo rõ câu bị cắt trước xuất; kiểm tra dịch/rút gọn còn giữ ý; thử gộp/chỉnh mốc có ngữ cảnh và so với baseline cố định. Không tự tăng tốc giọng hoặc bỏ từ để đạt tỷ lệ đẹp. Cần người nghe đánh giá mẫu tiền/tên/phủ định, đuôi câu và độ tự nhiên; công cụ ở lượt này chưa cung cấp đánh giá nghe.

Vẫn còn thiếu xuất toàn nháp 105 phút từ cache giọng thiếu, phát giọng liên tục toàn nguồn dài, ASR CPU toàn nguồn, lỗi CUDA runtime→fallback, cold disk, VRAM/peak RAM toàn vòng, installer/Windows sạch và ma trận mọi provider/ngôn ngữ. Không gọi toàn bộ các mục đó là đã đạt. Gói dùng thử vẫn là bản trước; chưa rebuild/publish hoặc thay đổi bộ cài trong lượt này.
