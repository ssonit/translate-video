# Nguồn thật và gói dùng thử — 31/10/2026

## Phạm vi mới

Repo bắt đầu sạch ở `da40a1d`, không reset, commit hoặc push. Bằng chứng mới nằm trong `.bench-asr/real-long-20261031/`; các báo cáo preview trước là bằng chứng lịch sử. Không sửa mã sản phẩm vì chưa tìm được lỗi sản phẩm tái hiện độc lập. Các lỗi chất lượng dưới đây là nội dung cache cũ, chưa xác định được dịch vụ/model đã tạo.

Đã probe tám MP4 lớn nhất trong `output/tai-ve` (`inventory.json`). Chọn `Build & Sell Grok Bots (2 Hour Course) [4hKJ9X6rGFo].mp4`: **1.806.307.428 byte, 6.341,094 giây (105:41), VP9 1920×1080, khoảng 30 fps**. Đây là nguồn thật lớn nhất trong thư mục nguồn của project, không phải tuyên bố đã tìm mọi ổ đĩa trên máy. File chưa tới 2 GB. Không tải video/model/voice.

`setup.py` chép transcript, lines, bản dịch và 3.158 WAV sang `jobs/…` riêng; nguồn MP4 chỉ đọc trực tiếp. Không chạy lại ASR vì đã có transcript thực tế. Không dùng WAV cũ như cache đã xác minh: mở lần đầu trả 0 ready và cảnh báo nguồn/cấu hình chưa được xác nhận. Nháp dùng dịch vụ local/giọng ngochuyennew riêng để thử; cấu hình người dùng ban đầu vẫn DeepSeek local và ngochuyennew. Đổi cấu hình nháp vô hiệu hóa bản dịch phụ thuộc đúng hợp đồng; không sửa dữ liệu gốc để làm cache được nhận.

## Số đo mới

WebView2 của EXE production `optimized-final.exe` từ lượt trước, CDP loopback 9223 với profile riêng. Không gắn hook vào mã sản phẩm. Giữ cửa sổ mở cho bridge/worker giọng. Build release mới được thực hiện riêng sau lượt chức năng.

| Thao tác | Số đo |
| --- | --- |
| `read_draft`, 4 lượt | 1.983,1; 1.670,1; 1.671,4; 1.686,9 ms |
| Hình đầu, media phụ sau IPC | 324,9 ms |
| Tua 3.000 → 6.000 → 30 giây, hai vòng | 207,7; 62,0; 46,0; 27,9; 43,4; 30,7 ms |
| Lưu một câu Việt mới | 1.458,1 ms |
| Nghe câu sửa, lần đầu | 8.723,5 ms |
| Nghe lại câu đã có cache | 1.684,6; 1.692,6 ms |
| Mở lại qua IPC sau hủy intent | 2.133 ms, giữ 3 ready |

Raw: `measure.json`, `edit.json`. Thời gian hình/tua là phần media phụ trong WebView thật, không phải tổng thời gian mở React editor. Mọi lượt là cache hệ điều hành ấm; không có cold disk. Build LTO và ứng dụng người dùng chạy cùng lúc, không coi đây là môi trường benchmark cô lập.

UI thật đã reload và hiện 3.158 câu/105:41; Nguồn phát được, Computer Use đọc accessibility và gửi click Tạm dừng. Ảnh CDP `editor.png` đã xem, UI nhận 3 ready sau reload; chế độ dịch/giọng bị khóa đúng khi thiếu model dịch. Không coi click Windows đơn lẻ là nghiệm thu trọn ma trận tua/pause bằng chuột; các lượt tua đo bằng phần tử video trong WebView. Chưa có lượt UI giọng liên tục trên nguồn dài khi dịch vụ đang sẵn sàng.

Ollama 11434 không kết nối được; DeepSeek local chưa chạy. Lượt đầu yêu cầu vùng dùng lời chưa authored ghi rõ lỗi kết nối (12,15–12,46 giây); hủy trả `Đã dừng.`, backend trở về `running:false`. Không tự bật dịch vụ. Lượt thử tiếp dùng lời authored, không gọi dịch. `regions-retry.json` chỉ là kiểm tra tạo cache chọn lọc với hậu tố thử sau khi cấu hình đã xóa bản dịch; **không dùng nó để đánh giá giọng dịch**. `edit.json` dùng câu Việt đầy đủ, được viết rõ trong `edit.js`.

## Xuất từ nháp thiếu

Tách ba đoạn nguồn dài 25 giây bằng stream copy, chỉ trong thư mục nghiệm thu: `dau.mp4` (40 giây), `giua.mp4` (3.170 giây), `cuoi.mp4` (6.300 giây). Ranh giới stream copy có thể lệch keyframe; chưa nghiệm thu đồng bộ hình/tiếng ở mức frame từ các đoạn này.

Nháp `short2` dùng đoạn đầu, 14 câu và lời Việt có sẵn từ cache gốc, đánh authored với dấu ba chấm để giữ nguyên nội dung chính khi cấu hình đổi. Không AI mới. Đã tạo trước 1 WAV rồi gọi `export_draft`: **12.316,6 ms**, kết quả `short2/dau.vi.mp4`, hoàn tất 14 WAV. Đây là phép xuất mới trên đoạn nguồn thật, không phải xuất toàn bộ 105 phút. `short`/`export.json` là lượt driver sai: mất lời khi cấu hình đổi rồi chỉ đọc dấu ba chấm; không tính kết quả đó là nghiệm thu. Driver đã sửa giữ lời trước configure, chạy lại thành `short2`/`export-valid.json`.

Lượt bổ sung trên `short2`: chụp SHA-256/mtime đủ 14 WAV, sửa lời câu cuối rồi xuất khi còn **13/14 ready**. Hoàn tất **7.555,9 ms** (`export-repeat.json`); chỉ `014.wav` thay hash/mtime, **13 WAV còn lại giữ nguyên**, không còn file tạm (`short-audit.json`). MP4 mới cũng giải mã toàn file không lỗi với FFmpeg `-xerror`.

Chưa xuất toàn bộ nguồn dài từ 3/3.158 WAV và chưa đo ASR toàn nguồn mới. Không lấy số 12,32 giây suy ra thời gian hoàn tất video dài.

## Chất lượng nội dung

Đã đọc đối chiếu 41 cặp câu nguồn/bản dịch quanh đầu, giữa, cuối trong `quality-samples.json` (số cặp chính xác xem file). Đây là đánh giá văn bản cache, chưa nghe hoặc xác nhận ASR bằng tai:

- 50,32–54,16 giây: nguồn tách `just under $5` / `million of top line revenue`; bản dịch `dưới 5 đô la.` / `triệu doanh thu hàng đầu` làm đơn vị tiền bị ngắt và dùng sai nghĩa top-line revenue. Cần duyệt chung hai câu để giữ “doanh thu gần 5 triệu đô la”.
- 41,68–43,28 giây: `And real quick, if you guys don't know me, my name is Nate.` thành `Tôi là Nate.`; mất lời dẫn. Không dùng mẫu này để khẳng định dịch đủ ý.
- 3.179,16–3.180,84 giây: phủ định `doesn't mean … perfect` được giữ trong `không có nghĩa … hoàn hảo`.
- 3.187,84 giây: `Vụ dụ` là lỗi chữ; 3.170 giây `friend` có thể là lỗi ASR/tên riêng, phải nghe nguồn để xác nhận, không tự đoán sửa tên.
- Cuối nguồn có cách gọi `Grok Bot`, đầu có `Grockbot/Grockbots`: cần thống nhất sau khi người nghe xác nhận. `audits` → `kiểm toán`, `retainers` → `hợp đồng bảo trì` có rủi ro sai nghĩa trong ngữ cảnh dịch vụ; cần duyệt ngữ cảnh.

Phiếu nghe còn chờ người: nghe ba MP4 mẫu cạnh JSON; đánh dấu tên/số/phủ định/thiếu ý; nghe `short2/dau.vi.mp4` và WAV 001 của nháp dài; ghi phát âm, tốc độ, đuôi câu bị cắt, lệch hình/tiếng và thời điểm cụ thể. Chấm từng mục đạt/lỗi/chưa rõ; không chấm chất lượng giọng nếu chưa nghe. Chưa có công cụ nghe được dùng trong lượt này, nên không khẳng định giọng tự nhiên, ASR đúng, nói nhanh không mất chữ hay đồng bộ đạt.

## Kiểm chứng

`npm run verify:local` mới: cả sáu nhóm đạt, 103 frontend; 340 Rust đạt, 7 ignored. Bằng chứng `.bench-asr/verify-1793435276830/report.json`. Chạy riêng nhóm `pipeline::video_encode::tests`: **4 đạt**, gồm cố ý làm GPU lỗi bằng chỉ số 99999 rồi encode CPU thật và giải mã; hủy/thành công không retry, lỗi CPU được trả. Raw `cpu.log`. Không tắt GPU/đổi driver toàn máy; không lấy đây làm nghiệm thu ASR CPU mới.

Lượt UI bổ sung `ui-actions.json`: bấm Nguồn/Phát, video tới 1,032319 giây; Tạm dừng giữ 1,032526 giây; slider React tua 3.000 → 6.000 → 30 giây, kết thúc đúng 30 giây, paused và không alert. Đổi nguồn đóng editor; reload mở lại đủ 3.158 câu/3 ready và video readyState 4 (`ui-reopened.json`). Đây là DOM của frontend production thật, bổ sung cho phép đo video phụ ở trên.

`post-audit.json` đối chiếu SHA-256, kích thước và mtime: nguồn và MP4 xuất gốc **giữ nguyên**; bản sao giữ nguyên 3.156/3.158 WAV, hai file khác là 001 và 1580. Snapshot ban đầu được lấy trong lúc lượt tạo giọng chọn lọc còn chạy, nên không dùng so sánh này để khẳng định *riêng thao tác sửa một câu* chỉ đổi một WAV. Không có `.partial.mp4`, `dub.wav`, `.fit.wav` trong nháp dài. Phép so WAV trước/sau xuất có kiểm soát được thực hiện riêng ở mẫu `short2` nêu trên.

180 mẫu cây tiến trình EXE/WebView, khoảng một giây/mẫu trong giai đoạn sửa/nghe/xuất mẫu: peak tổng working set **1.127.829.504 byte**, private bytes **910.614.528 byte** (`ram.jsonl`, `memory-summary.json`). Working set có thể đếm trùng trang chia sẻ; private bytes không phải RAM resident; chuỗi này không bao phủ mọi lượt từ lúc mở app. VRAM `nvidia-smi` toàn GPU là 2.291 MiB trước và 1.969 MiB ở một thời điểm thử trên RTX 4060 Laptop 8.188 MiB; không phải peak hoặc VRAM riêng app, không suy ra tối ưu bộ nhớ từ hai mẫu này.

Đã chạy Whisper CLI thật với `-ng`, ggml-small hiện có, trên 10 giây tiếng tách từ đoạn đầu. Log xác nhận `use gpu = 0`, CPU backend, JSON chép lời được ghi; thời gian nội bộ **6.450,21 ms** (`cpu-asr.log`, `cpu-asr.json`). Không đổi GPU hệ thống/model, không gọi mạng. Đây là phép chạy CPU trực tiếp, chưa phải nghiệm thu app tự phát hiện GPU không dùng được và chuyển ASR trên nguồn dài.

Các giới hạn còn lại: dịch mới khi provider sẵn sàng; ASR CPU/GPU toàn video; phát liên tục/chuyển mode/tua giọng qua UI dài; xuất toàn bộ nháp dài thiếu cache; cold disk; đánh giá nghe bằng người; installer và Windows sạch. Không đánh dấu đạt bằng test hoặc bằng chứng cũ thay thế.

## Bản dùng thử

Build mới `npm run tauri build -- --no-bundle -- --offline` thành công, release LTO mất 9 phút 32 giây (không phải thời gian xử lý video). Frontend có cảnh báo chunk lớn; build không lỗi. EXE chạy lượt chức năng phía trên có hash `6D5458BAA7F5885E7304223D1314A3C2EE705A53CF1AD359648E5C0126E7FC9C`; không coi toàn bộ lượt đó đã chạy lại trên EXE mới.

Gói: `output/windows-20261031-real-video/`; ZIP: `output/windows-20261031-real-video.zip`. Chạy `scripts/package-windows.ps1` trên EXE mới rồi `check-windows-release.ps1`: `manifestPassed:true`, không thiếu file bắt buộc hoặc file chưa kê (`preflight.json`). Gói chỉ có tài nguyên theo cấu hình hiện tại, không kèm model, giọng hay key. Không ký/publish.

| Artifact | SHA-256 |
| --- | --- |
| `tauri-app.exe` | `58F7D8A89DF820F574963AB2392C7DBCB2542ECF9316B171F32918A5FDD854DF` |
| `windows-20261031-real-video.zip` | `2727F2A4EE88B7F51E22F10F28D725E42B0DCAF91686B813A8604B1CD24E60BF` |

Không thấy `makensis`, `candle`, `light`, `VBoxManage`, `vmrun` trong PATH; không có NSIS tại hai vị trí Program Files, WiX v3.14 hoặc cache Tauri tại các vị trí đã kiểm tra (`installer-tools.json`). Không tự tải bộ đóng gói, không chạy installer lên máy người dùng. Chưa có môi trường Windows sạch được xác nhận cho lượt này; checklist cài/mở/gỡ/cài lại và chuẩn bị đồ nghề nằm trong `docs/windows-release.md` đi kèm gói dưới tên `HUONG-DAN.md`. EXE được mở riêng trên máy phát triển để kiểm tra khởi động; đây không phải nghiệm thu installer/máy sạch.

Computer Use đọc được cửa sổ của EXE bàn giao tại đúng đường dẫn gói: vào màn Lồng tiếng, chip “Thiếu 3 mục, xem”, báo thiếu Model dịch/Giọng Việt/Whisper và hướng dẫn thêm cặp voice vào voices/vi. Không chép tài nguyên của phiên benchmark vào gói để vượt kiểm tra thiếu. Hai tiến trình thử đã dừng theo PID và kiểm tra đúng đường dẫn EXE trước khi dừng; không dừng dịch vụ/người dùng. Profile CDP và driver chỉ nằm trong thư mục bằng chứng; không đổi `src/main.tsx`, CSP hoặc ACL.

Tái lập phép đo: chạy `setup.py` một lần trên thư mục thử mới, `measure.js` qua harness `../preview-release-20261031/cdp.mjs`; tiếp theo đọc driver trước khi chạy `regions.js`, `edit.js`, `short.py`/`export.js` vì chúng có thay đổi nháp thử. Giữ nguyên nguồn và chỉ trỏ root vào fixture riêng. Các driver phụ thuộc template local/giọng đã có của máy này, không phải bộ benchmark portable. `audit.py`/`post.py` giữ hash/mtime và các trích đoạn; `short-before.json`/`short-audit.json` là bằng chứng kiểm soát WAV của lượt xuất lại. Không bật CDP trên bản dùng thường ngày. Dừng riêng hai tiến trình thử khi hoàn tất, giữ lại fixture và báo cáo để người dùng xem/nghe.
