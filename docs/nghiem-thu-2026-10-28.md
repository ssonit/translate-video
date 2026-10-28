# Nghiệm thu video thật ngày 28/10/2026

## Phạm vi và dữ liệu

Dùng lại nguồn nói thật `output/kiem-thu-20261028/hometown-75s.mp4` và cache 16 câu từ lượt E2E trước. Tạo các folder việc riêng trong `output/nghiem-thu-20261028/`, giữ nguyên dữ liệu cũ. Giọng đang chọn là `ngochuyennew`; không đổi giọng hay dịch vụ đã lưu của người dùng, không tải đồ nghề. Các bước chép lời, tên, thuật ngữ và dịch dùng cache; lượt này không xác nhận lại chất lượng Whisper hay dịch vụ dịch.

Các lệnh `start_dub`, `render_sample`, `preview_voice`, `cancel_dub` chạy qua IPC thật trong cửa sổ Tauri dev. Hàng đợi dùng chính `jobActions` với trạng thái kiểm thử trong bộ nhớ, gọi IPC thật và gặp lỗi FFmpeg thật; không thay lỗi bằng IPC giả. Lượt này chưa kiểm tra thao tác bấm từng nút hàng đợi trên giao diện. Kịch bản chỉ gắn tạm vào entry của dev, được tháo trước khi build bàn giao. Hai khóa lưu hàng đợi/lịch sử được khôi phục sau lượt chạy.

Log, kịch bản và số đo ở `.bench-asr/acceptance-20261028/`: `events.jsonl`, `comparison.json`, `audit.json`, `acceptanceDriver-first.ts` và bản kịch bản kiểm tra lại. Đây là dữ liệu local được Git bỏ qua.

## Trộn âm và cách đọc tên

Đã xuất bốn bản đầy đủ có phụ đề, khung 9:16, dài khoảng 75,033 giây, audio stereo:

| Folder | Cấu hình |
| --- | --- |
| `voice-only` | Tiếng gốc 0%, giọng Việt 100%. |
| `mix-flat` | Tiếng gốc 60%, giọng Việt 100%, không tự hạ. |
| `mix-duck` | Tiếng gốc 60%, giọng Việt 100%, tự hạ còn 22% mức tiếng gốc khi giọng đọc chạy. |
| `pronunciation` | Như `mix-duck`, thêm cách đọc thử Toronto → Tô rôn tô, Niagara → Ni a ga ra. |

Mỗi folder chứa `hometown-75s/hometown-75s.vi.chu.mp4`. Các cách đọc thử chỉ nằm trong việc nghiệm thu, không áp dụng vào bộ cài đặt của người dùng.

- [Nghe ba cách trộn](../output/nghiem-thu-20261028/nghe-tron-am.wav): chỉ giọng Việt → trộn không hạ → trộn có hạ; mỗi đoạn 8 giây, cách nhau 1 giây.
- [Nghe hai cách đọc tên](../output/nghiem-thu-20261028/nghe-ten.wav): nguyên chữ Toronto/Niagara → cách viết phiên âm thử, cách nhau 1 giây.
- [Bản có tự hạ tiếng gốc](../output/nghiem-thu-20261028/mix-duck/hometown-75s/hometown-75s.vi.chu.mp4).

Đổi mức trộn giữ nguyên SHA-256 của toàn bộ 16 WAV. Đổi cách đọc chỉ thay `016.wav`; 15 WAV còn lại, lời chép, bản dịch và SRT giữ nguyên SHA-256. Bản đổi cách đọc tái sử dụng đúng 2.250 gói video đã mã hóa của bản tham chiếu. Các bản hoàn tất không còn `audio.wav`, `dub.wav`, `*.fit.wav` hay `*.partial.mp4`.

**Chưa chốt đạt phần nghe bằng người.** Các file trên là mẫu để người dùng xác nhận độ rõ, cân bằng và cách đọc tên. Nguồn này chưa đại diện nhạc nền lớn, nhiều người nói hay nói nhanh. Không dùng số đo hay chép lại bằng ASR để khẳng định giọng tự nhiên.

## Đoạn thử 8 giây so với bản đầy đủ

Gọi `render_sample` tại giây 65,7, dài 8 giây, đi qua đoạn có Toronto/Niagara; không chép lời, dịch hoặc tạo lại giọng cho đoạn thử. Cả bốn cặp đều giải mã hết bằng FFmpeg không báo lỗi; đoạn thử dài đúng 8 giây.

| Bản | Tương quan âm sau căn thời gian | SSIM hình |
| --- | ---: | ---: |
| Chỉ giọng Việt | 0,9961 | 0,9921 |
| Trộn không hạ | 0,9946 | 0,9921 |
| Trộn có hạ | 0,9959 | 0,9921 |
| Cách đọc thử | 0,9955 | 0,9921 |

Âm của đoạn thử sớm hơn bản đầy đủ khoảng 21,31 ms khi so cùng mốc nguồn, gần một khung AAC ở 48 kHz. Hai đường xuất có cách xử lý đệm AAC khác nhau; không kết luận chúng đồng nhất từng mẫu. Tương quan đo sau căn độ lệch và bỏ 0,2 giây ở hai đầu; SSIM đo trên ảnh thu nhỏ 270×480. Các số này chỉ mô tả lượt thử, không phải ngưỡng nghiệm thu cho mọi nguồn. Ảnh mẫu đã xem có cùng bố cục dọc và phụ đề Việt, giữ chữ tiếng Anh có sẵn trong nguồn.

Các bản sao đoạn thử có tên ổn định ở `output/nghiem-thu-20261028/`: `voice-only-8s.mp4`, `mix-flat-8s.mp4`, `mix-duck-8s.mp4`, `pronunciation-8s.mp4`.

## Lỗi tìm được ở hàng đợi

Lượt đầu: `broken.mp4` là fixture không có cấu trúc MP4, FFmpeg báo `moov atom not found`. Chính sách dừng hoạt động, nhưng sau khi gọi **Chạy tiếp** để chạy video hợp lệ đang chờ, `takeActive` tự đưa video lỗi cũ vào hàng một lần nữa.

Sửa `runQueue` trong `src/work/job.ts`: khi người dùng chạy danh sách chờ và việc hiện tại trùng lượt lỗi cuối trong lịch sử, bỏ trạng thái hiện tại trước khi lấy việc tiếp theo. Lượt lỗi vẫn ở lịch sử để **Xếp lại lượt này**; **Thử lại** vẫn có cấu hình đã giữ. Không tự xác nhận tên. Kiểm thử hồi quy trong `scripts/test-queue-actions.mjs` kiểm tra cả chuỗi dừng → chạy tiếp, thay vì chỉ kiểm tra thời điểm vừa dừng.

Kiểm tra lại qua IPC thật sau sửa:

| Tình huống | Kết quả |
| --- | --- |
| Dừng khi lỗi, rồi Chạy tiếp | Lịch sử `failed → done`, hết việc chờ; không chạy lại file hỏng. |
| Tiếp tục khi lỗi | Lịch sử `failed → done`, video hợp lệ xuất xong. |
| Bỏ qua khi đang ghép | Gửi hủy lúc bước ghép bắt đầu, rồi nguồn khác chạy xong; lịch sử `skipped → done`, không còn `.partial.mp4`. |

Fixture bỏ qua được sửa để dùng hai nguồn khác nhau: ban đầu kịch bản chèn cùng một nguồn hai lần, trái quy tắc chống trùng của hàng đợi. Không tính lần đó là lỗi sản phẩm. Lượt kiểm tra lại dùng `hometown-75s.mp4` và `hang-a.mp4`, trong các folder kết quả riêng. Không có sự kiện yêu cầu duyệt tên trong các lượt cache này.

Sau tháo kịch bản: 81 kiểm thử frontend và 3 kiểm thử `jobActions` qua, bao gồm thao tác chủ động xếp lại video lỗi từ lịch sử. Không sửa Rust nên không dùng lượt này để tuyên bố đã chạy lại 244 kiểm thử Rust.

`npm run tauri build -- --no-bundle -- --offline` đã qua cả TypeScript/Vite và desktop release. Đã kiểm tra không có tên kịch bản, URL collector hoặc tên dữ liệu nghiệm thu trong JavaScript production. `src/main.tsx` được khôi phục đúng từng byte; không commit.

## Windows và bàn giao

EXE release trước sửa đã mở được trên máy hiện tại, vào màn lồng tiếng và báo thiếu Model dịch/Giọng Việt/Whisper đúng như cách cô lập tài nguyên release. Đây không phải thử trên Windows sạch.

Thêm `scripts/check-windows-release.ps1`; script đóng gói chép nó vào gói và tính cả file này trong manifest. Script kiểm tra SHA-256/kích thước, file bắt buộc, WebView2 và sự có mặt công cụ trong PATH, không tải hay chạy chúng. Đã thử trên Windows PowerShell 5.1 với manifest đúng, file bị thay đổi, đường dẫn vượt folder và đường dẫn trùng. Hướng dẫn có bảng kiểm riêng cho máy sạch ở [Windows release](windows-release.md).

Người dùng xác nhận chưa có máy/VM Windows sạch. Chưa có lượt cài/gỡ installer NSIS/MSI được xác nhận; không tự tải hay chạy installer. Gói bàn giao vẫn là EXE kèm tài nguyên, không coi là bộ cài đã nghiệm thu. Các dịch vụ API, nguồn link, ngôn ngữ khác và đường dẫn không có quyền ghi vẫn cần lượt riêng.

Bản bàn giao sau sửa nằm ở `output/windows-20261028-nghiem-thu/`, bản ZIP cùng tên ở `output/`. Gói cũ `output/windows-20261028/` giữ nguyên. Giải nén cả folder và chuẩn bị đồ nghề theo `HUONG-DAN.md`; không lấy riêng EXE.

Đã mở EXE mới từ chính folder bàn giao, xác nhận vào màn chính và báo thiếu đồ nghề như bản trước. Manifest của 12 file khớp; đã kiểm tra lại nội dung từng file bên trong ZIP theo manifest. EXE mới khác SHA-256 với bản cũ; SHA-256 bản mới là `174e2310a22b154fac94e80fe0bf2e2b7ecbb1485723142e5e949e3b50249b93`. Báo cáo kiểm tra gói ở `.bench-asr/acceptance-20261028/final-package-preflight.json`.
