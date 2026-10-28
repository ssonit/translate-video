# Bản Windows chạy thử

Build frontend: `npm run build`. Build chương trình desktop: `npm run tauri build -- --no-bundle -- --offline`. Lệnh thứ hai dùng dependency đã có trên máy, không tạo installer. Đóng gói EXE và các script cần thiết bằng `powershell -File scripts/package-windows.ps1`; script chỉ ghi vào folder mới và tạo manifest SHA-256.

Mở `tauri-app.exe` trong folder bàn giao. Tên sản phẩm và identifier vẫn là placeholder theo yêu cầu. Bản này cần WebView2 Runtime đã có trên Windows. Đừng di chuyển riêng EXE rồi bỏ các folder `scripts` và `douyin` đi.

Trước khi mở bản bàn giao, có thể kiểm tra file và môi trường bằng Windows PowerShell:

```powershell
powershell -NoProfile -File .\check-windows-release.ps1 -ReportPath .\preflight.json
```

Chạy lệnh trong folder bàn giao. Script chỉ đọc manifest SHA-256, các file đã kê, phiên bản WebView2 và kiểm tra công cụ trong PATH; không chạy công cụ, tải, cài hay đọc key/phiên đăng nhập. `manifestPassed: true` xác nhận các file đã kê còn đúng kích thước và SHA-256. Mã thoát 1 nghĩa là có file thiếu hoặc không khớp; lỗi manifest cũng dừng với mã lỗi. Báo cáo chỉ ghi vào tên file mới, không ghi đè. Nếu chạy từ mã nguồn, truyền thêm `-Delivery <folder bàn giao>`. Kết quả này không xác nhận máy sạch, model/giọng, chất lượng tiếng hay installer.

Script cũng kiểm tra toàn bộ module Douyin và chính script kiểm tra gói có mặt, được kê trong manifest. `unlistedRequiredFiles` liệt kê file bắt buộc chưa được kê; trường hợp này làm `manifestPassed` thành false. Manifest thiếu SHA-256/kích thước hợp lệ, trùng đường dẫn hoặc vượt folder bị từ chối.

## Kiểm tra lại từ mã nguồn

Chạy `npm run verify:local` trên Windows để kiểm tra frontend, thao tác hàng đợi, script kiểm tra gói, build frontend và Rust offline. Từng bước có log riêng và kết quả chung tại `.bench-asr/verify-<mốc thời gian>/report.json`; lệnh trả mã lỗi nếu bất kỳ bước nào lỗi. Lệnh tiếp tục các bước độc lập để báo đủ kết quả. Cần dependency đã cài, Node hỗ trợ TypeScript type stripping, Windows PowerShell và Rust; FFmpeg/ffprobe cần có để các bài thử media chạy đầy đủ. Lệnh không gọi API dịch, tải nguồn link, chạy model hoặc sửa cấu hình người dùng. Chạy riêng kiểm tra gói bằng `npm run test:release`.

## Lượt thử trên Windows sạch

Hiện chưa có máy/VM sạch để thực hiện. Khi có máy thử, giữ nguyên folder bàn giao và ghi lại các mục sau; không tính máy phát triển là kết quả thay thế:

| Thứ tự | Thao tác | Điều cần xác nhận |
| --- | --- | --- |
| 1 | Ghi phiên bản Windows, chạy script kiểm tra gói | Manifest khớp; ghi rõ WebView2 và đồ nghề đang có. |
| 2 | Mở EXE khi chưa thêm model/giọng | Vào được màn lồng tiếng; mục thiếu hiện rõ, nút chạy khóa đúng. Nếu thiếu WebView2, ghi nhận riêng để kiểm tra chính sách runtime của installer sau này. |
| 3 | Tự chuẩn bị đồ nghề theo phần bên dưới, mở lại app | Chip phản ánh đúng; không lấy tài nguyên từ máy build hay tự tải thêm. |
| 4 | Chọn video nói thật với tên file và đường dẫn tiếng Việt | Xuất video, nghe tiếng, xem chữ, lưu ra folder khác và mở lại kết quả. |
| 5 | Chọn nơi không có quyền ghi | Báo lỗi rõ, không để bản xuất hỏng được nhận là hoàn tất. |
| 6 | Thêm một video hỏng và một video hợp lệ vào hàng đợi | Thử Dừng khi lỗi, chạy tiếp, Tiếp tục khi lỗi và Bỏ qua; việc sau vẫn chạy, lịch sử đúng. |
| 7 | Khi đã có installer được build bằng công cụ được chuẩn bị sẵn | Cài, mở từ shortcut, xuất video, gỡ rồi cài lại; xác nhận tài nguyên đi kèm còn đủ và dữ liệu người dùng được xử lý đúng. |

Ghi kết quả thực tế, lỗi và đường dẫn báo cáo cho từng mục. Chỉ đánh dấu installer đạt sau khi hoàn tất mục 7 trên máy sạch.

## Đồ nghề

Bản bàn giao gồm EXE, frontend/font/WASM nhúng trong EXE, script faster-whisper và mã Douyin. Không kèm model Whisper, giọng, Piper, FFmpeg, Python, Ollama, yt-dlp, Edge, DeepSeek local, API key hay phiên đăng nhập của máy phát triển. Không có thao tác tự tải/cài các phần này khi mở app.

Trong bản release, bấm **Mở thư mục đồ nghề** tại màn Đồ nghề để mở `%LOCALAPPDATA%/com.khung.studio`. Đặt các folder sau vào đó; app cũng nhận folder tương ứng cạnh EXE:

- `voices/vi/`: cặp ONNX và ONNX.json của giọng đã chọn; ngôn ngữ khác đặt trong folder mã ngôn ngữ tương ứng. Chỉ dùng giọng người dùng đã chuẩn bị.
- `whisper/`: whisper-cli.exe, DLL đi cùng và ggml-*.bin; chấp nhận thư mục con Release. Nút Tải Whisper trong app vẫn là ngoại lệ tải duy nhất, ghi vào thư mục dữ liệu có quyền ghi.
- `piper/`: tùy chọn, piper.exe và espeak-ng-data. Thiếu thì giọng chạy bằng Web Worker trong cửa sổ app.
- `Deepseek-API/`: tùy chọn, bản server của chính người dùng; không sao chép phiên đăng nhập từ máy khác. Bật/tắt bằng nút trong app.

FFmpeg/ffprobe trong PATH. yt-dlp chỉ cần với YouTube/TikTok. Douyin dùng Edge headless và Python đã cài. Đường GPU faster-whisper cần Python, faster-whisper/CUDA và `whisper/large-v3-turbo/model.bin` đã được người dùng chuẩn bị. Ollama chỉ cần khi chọn dịch trên máy; giữ hai model đã hướng dẫn trong README. Kết quả mặc định của release nằm trong thư mục dữ liệu ứng dụng; đổi nơi lưu trong Cài đặt.

## Trạng thái xác nhận ngày 28/10/2026

Lượt cuối trước push: gói `output/windows-20261028-prepush/` và ZIP cùng tên đã build, đối chiếu manifest/ZIP đủ 12 file và mở EXE cuối trên máy phát triển. Đã kiểm tra điều hướng, khóa chạy khi thiếu đồ nghề và đọc/phát bản lịch sử. Xem [Rà soát trước push](ra-soat-truoc-push-2026-10-28.md) để biết hash, bằng chứng và lỗi bất đồng bộ đã sửa. Đây vẫn là gói EXE kèm tài nguyên, chưa phải installer nghiệm thu trên máy sạch.

Đã qua kiểm thử tự động trên máy phát triển: pipeline/cache, FFmpeg, trộn âm, logo/vùng che và đoạn thử, cùng luồng hàng đợi có lỗi/bỏ qua. Chi tiết ở `docs/nang-cap-2026-10-28.md` trong mã nguồn.

Chưa xác nhận installer NSIS/MSI, Windows sạch không có dependency, mọi dịch vụ dịch, đủ bốn ngôn ngữ nguồn, và mọi nguồn link. Công cụ đóng gói installer chưa có trên máy này; không tự tải để vượt giới hạn chuẩn bị đồ nghề. Muốn phát hành installer cần môi trường build đã được người dùng chuẩn bị NSIS/WiX rồi chạy `npm run tauri build`, kiểm tra trên máy/VM Windows sạch và hoàn tất bảng kiểm còn lại trong kế hoạch. Không coi EXE chạy được ở máy phát triển là đã đạt bước này.
