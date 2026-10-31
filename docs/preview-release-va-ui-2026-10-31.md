# Preview dài: release và WebView Tauri — 31/10/2026

## Thay đổi

`draft::status` vẫn đọc đủ nội dung WAV, so SHA-256 với receipt và kiểm tra PCM hoàn chỉnh. Nó giữ thêm thời lượng trong `speech_seconds` của chính snapshot đó. `speech_slots_verified` dùng thời lượng này; chỉ đường legacy chưa có manifest mới đọc WAV riêng. Trường mới có `serde(skip)`, không thay schema nháp, receipt hay IPC. Snapshot không được lưu xuyên thao tác. Lần gọi sau vẫn xác minh nội dung, kể cả sửa PCM giữ nguyên kích thước file.

Đã sửa thêm lỗi thấy khi thử UI: phản hồi lỗi của vùng đã bị thay bởi tua/pause có thể làm vùng mới ngừng phát và hiện “Vùng yêu cầu đã đổi.”. Queue chỉ báo lỗi nếu serial vẫn là yêu cầu đang phát. Vẫn nhận cache đã chốt của tác vụ cũ theo hợp đồng hiện có; lỗi của yêu cầu hiện tại không bị nuốt. Hai test mới kiểm tra tua, pause và lỗi hiện tại.

Lượt tua nhanh kế tiếp còn bắt được DOMException `AbortError` do `play()` bị `pause()` khi tua/đợi giọng. Đã bỏ báo lỗi/dừng ý định phát đối với ngắt dự kiến này, hoặc khi cửa sổ/ý định đã đóng. Vẫn báo lỗi quyền phát (`NotAllowedError`), giải mã (`NotSupportedError`) và lỗi thật khác. Test phân biệt đủ các trường hợp; WebView xác nhận DOMException thuộc Error trong runtime này.

Không đổi model, voice hay provider đã lưu của người dùng. Các lệnh cấu hình thử chỉ tác động nháp trong `.bench-asr/preview-release-20261031/`. Giọng `ngochuyennew` được chép từ chính project vào thư mục EXE thử; junction Whisper trỏ đúng thư mục Whisper của project. Không tải công cụ/model/giọng, không gửi media hay lời thử qua API.

## Backend release

Máy và toolchain như báo cáo debug `do-preview-dai-2026-10-31.md`. Lệnh baseline: `cargo test --release --offline benchmark_preview_components -- --ignored --nocapture --test-threads=1`. Ba lượt: status trung vị **527,51 ms**, khoảng 485,58–584,69; clips **535,47 ms**, khoảng 450,83–564,45. Đây là profile release, khác đáng kể số debug trước đó. Dữ liệu: `preview-release-baseline-2026-10-31.json`.

Lệnh so cặp: `cargo test --release --offline benchmark_preview_duration_snapshot -- --ignored --nocapture --test-threads=1`. Fixture tạm riêng: 2.185 câu, 192.813.140 byte WAV, lời cố định. Chín cặp đảo thứ tự. Nhánh trước mô phỏng lần đọc lại PCM, đối chiếu thời lượng từng câu với snapshot; nhánh sau dùng trực tiếp snapshot. Cả hai chạy cùng mã xác minh hash/PCM và cùng cách dựng clips. Không gọi AI, TTS, FFmpeg hay IPC trong đồng hồ này.

| Đường | Trung vị ms | Nhỏ nhất–lớn nhất ms |
| --- | ---: | ---: |
| Đọc lại WAV lấy thời lượng | 1.086,70 | 1.057,12–1.214,89 |
| Dùng thời lượng snapshot | 670,26 | 609,29–734,26 |

Giảm trung vị **38,32%** ở phạm vi backend này. Đây là nhánh mô phỏng đọc thừa trong test mới, không thay cho đo EXE trước/sau. Dữ liệu đủ 18 lượt trong `preview-duration-release-2026-10-31.json`. Lượng đọc WAV logic trong một snapshot + clips giảm 385.626.280 → 192.813.140 byte; không phải số byte đọc đĩa vật lý.

Fixture vừa được tạo: mọi lượt là cache file ấm, kể cả lượt đầu. Không xóa standby cache toàn máy. Không có số cold disk.

## WebView release thật

Build bằng `npm run tauri build -- --no-bundle -- --offline`. Baseline EXE SHA-256 `3AB1813BB9387849A147A1C181219A6E2192CA96486354BEFA1F8EC74DC11BDE`, giữ riêng dưới `.bench-asr/preview-release-20261031/baseline.exe`. Bản tối ưu WAV giữ ở `optimized-wav.exe`. Bản này còn frontend trước sửa lỗi serial; kết quả đo WAV và lỗi UI được tách riêng.

Điều khiển đo qua CDP của WebView2 trên loopback 9223 (`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` của tiến trình thử), gọi IPC thật và kiểm tra DOM/media của frontend build production. TTS chạy bridge/worker thật. Hook chỉ ở phiên debugger và harness thử, không chèn vào `src/main.tsx` hoặc bản sản phẩm. Không mock dịch vụ bằng máy chủ; lời fixture cố định hoặc Không dịch.

Nguồn UI tổng hợp dài 6.555 giây (109:15), 320×180, 1 fps, file chỉ **105.198 byte**, màu xanh. Đây là phép thử nhiều câu/WAV và timeline dài, không đại diện chi phí băm nguồn nhiều GB hay giải mã video bitrate cao. Nháp có đủ 2.185 WAV PCM hợp lệ. Bản sao fixture dùng ID/receipt và lời cố định; sau khi mở/import, các câu được đánh authored để không gọi model dịch.

### Mở và tới hình

Baseline `read_draft` qua IPC thật: lượt đầu tiến trình **15.934,1 ms**, bốn lượt tiếp **491,8–526,5 ms**. Không gọi lượt đầu này là cold disk và chưa xác định nguyên nhân outlier. Một nhóm lặp sau: 503,0–600,7 ms. Bản tối ưu: 520,0–577,2 ms. Không thấy bằng chứng cải thiện mở nháp; tối ưu này không bỏ xác minh nguồn/WAV ở lúc mở.

Video phụ gắn trong WebView giải mã hình đầu sau 26,9 ms baseline; lặp lại 18,8 ms; bản tối ưu 2,0 ms. Tua tới giây 6.000 mất 4,7–9,7 ms baseline và 5,7 ms bản tối ưu. Đây là phần media sau IPC, không phải thời gian tổng mở editor.

Reload frontend thật có quan sát DOM đủ 2.185 câu và video có dữ liệu. Năm lượt baseline: hình đầu khoảng 747–1.948 ms từ navigation; editor khoảng 771–2.102 ms. Năm lượt tối ưu: hình đầu khoảng 856–2.088 ms; editor khoảng 833–2.105 ms. Một phần dùng kiểm tra trạng thái theo chu kỳ 250 ms nên là cận trên có độ phân giải đó, phần còn lại dùng loadeddata/RAF. Có build release khác chạy nền trong thời gian đo. Không dùng hai nhóm reload này để kết luận tăng tốc hay hồi quy mở màn; không thấy lợi ích ổn định. Dữ liệu từng lượt: `baseline-render*.json`, `optimized-render-*.json` trong thư mục bằng chứng.

### Cập nhật vùng và IPC

Năm yêu cầu tuần tự vùng 6.000–6.012 giây, nháp đầy đủ cùng dữ liệu, đi từ JavaScript WebView tới Rust rồi nhận IPC/sự kiện:

| Mốc | Baseline ms, trung vị (khoảng) | Tối ưu WAV ms, trung vị (khoảng) |
| --- | ---: | ---: |
| Sự kiện preview đầu | 1.257,1 (1.182,4–1.293,0) | 856,3 (846,6–873,6) |
| Promise vùng hoàn tất | 2.766,2 (2.660,7–2.958,7) | 1.950,0 (1.936,9–1.956,5) |

Giảm khoảng 31,9% tới sự kiện và 29,5% tới promise trong các nhóm này. Các nhóm EXE chạy tuần tự, không đảo thứ tự như benchmark backend; compiler chạy nền là nhiễu. Đây là IPC trong WebView, không phải toàn bộ thời gian React vẽ xong hoặc tốc độ lồng video. Handler đo dùng owner riêng nên editor không áp sự kiện owner này. Lượt chế độ thật trên fixture 16 câu được ghi riêng.

Đo bổ sung thứ tự **A–B–B–A**, mỗi nhóm mở một tiến trình mới và đo năm lượt trên cùng fixture/cache ấm, cùng profile WebView thử riêng. Tất cả response có 2.185 clips và payload như nhau. Gộp mười lượt mỗi bản:

| Mốc | Baseline trung vị (khoảng) ms | Tối ưu WAV trung vị (khoảng) ms |
| --- | ---: | ---: |
| Sự kiện đầu | 1.131,9 (1.064,0–1.719,8) | 817,4 (787,9–971,5) |
| Promise hoàn tất | 2.426,1 (2.351,1–3.089,1) | 1.863,6 (1.775,0–3.959,5) |

Đây là kết quả IPC dùng để kết luận chính: giảm median khoảng **27,8%** tới sự kiện và **23,2%** tới promise. Giữ nguyên outlier 3.959,5 ms của bản tối ưu, không loại khỏi thống kê; sự kiện đầu của lượt đó vẫn 817,2 ms. Chưa có trace xác định nguyên nhân phần đuôi. Build LTO vẫn chạy nền, nên không coi đây là môi trường benchmark cô lập hay bảo đảm worst case. Raw `preview-ipc-abba-2026-10-31.json` và `abba-*.json`.

Payload sự kiện thật **951.518 byte**, response cuối **951.492 byte**, gồm đủ 2.185 cues/clips. Một sự kiện mỗi nhóm dịch (tối đa sáu câu), một sau mỗi câu đọc mới, cộng response cuối. Lượt vùng đã có cache vẫn gửi snapshot. Payload thử backend 1.010.333 byte; chi phí đọc document + dựng JSON value + tuần tự hóa là **19,39–30,11 ms**, không phải chỉ thời gian serde hay thời gian vận chuyển IPC. Ở fixture 16 câu, chín cập nhật quan sát trong lượt chuyển text/voice mang 5.194–6.365 byte mỗi sự kiện.

Giữ giao thức đầy đủ: phần dựng/serialize đo được nhỏ hơn nhiều so với xác minh toàn việc; chưa có bằng chứng kiểm soát đủ để đổi sang delta có revision/reconnect riêng. Editor vẫn có đủ câu, timeline dùng đủ ngữ cảnh. Không tuyên bố delta không có ích cho mọi video; số byte xấp xỉ 1 MB/cập nhật vẫn là chi phí còn lại. Test hiện tại giữ bảo vệ owner, close và hàng đợi bounded; test mới bảo vệ lỗi serial cũ. Không thêm cache size/mtime hay bỏ hash để giảm payload.

## Kiểm chứng và giới hạn

Fixture chức năng là bản sao riêng của dữ liệu nghiệm thu 75 giây/16 câu ngày 29/10. IPC release thật: lưu một câu 700,4 ms; nghe câu sửa 5.247,7 ms (worker đầu); đổi mốc 359,9 ms; vùng chữ 401,6 ms; mở lại 234,2 ms. Chỉ `001.wav` khác hash bản sao ban đầu sau nghe một câu; các WAV khác giữ nguyên. Lưu/đổi mốc không tạo `dub.wav`. Đây là một lượt, không phải median TTS.

Xuất video từ bản sao còn **9/16 WAV hợp lệ** hoàn tất sau **41.627,9 ms**, dùng lời gốc cố định/Không dịch, không ghi chữ. FFmpeg `-xerror` giải mã toàn kết quả thành công. Cả chín WAV hợp lệ giữ nguyên SHA-256 **và mtime**; bảy câu thiếu được hoàn tất. Không còn `dub.wav` hoặc `.fit.wav`. So 16 clip preview sau xuất với 16 khoảng `voice-spans.json` thực tế: sai khác lớn nhất của đầu/cuối **0 giây**. Bằng chứng `pre-export.json`, `export-before-hashes.json`, `export-result.json`, `export-clips.json`, `export-audit.json`. Đây là lượt xuất mới của bản tối ưu WAV, không dùng lại con số nghiệm thu 29/10.

Đã bấm các nút của frontend qua DOM WebView: Nguồn phát tiến được; chế độ chữ phát video khi WAV chưa sẵn sàng; chế độ giọng giữ video khi đang chuẩn bị và tạo WAV từng câu. Baseline lộ lỗi vùng cũ nêu ở đầu báo cáo; không ghi lượt baseline này là nghiệm thu thành công chuyển mode. Đã chạy lại EXE cuối sau khi sửa cả serial và AbortError, với kết quả dưới đây.

### Nghiệm thu EXE cuối sau cả hai sửa lỗi

Đã build và chạy lại `optimized-final.exe`, SHA-256 **`6D5458BAA7F5885E7304223D1314A3C2EE705A53CF1AD359648E5C0126E7FC9C`**, asset frontend `index-xvWBb482.js`. Dùng profile WebView thử riêng, bản sao `functional-acceptance`, cố ý để thiếu WAV 12–16; giữ 11 WAV đầu đã xác minh.

- Bấm Nguồn → Phát: video tiến tới 1,116 giây. Chuyển chữ → Phát: tới 2,599 giây; giọng → Phát: tới 4,654 giây. Hai chế độ sau không có alert lỗi.
- Tua slider liên tiếp 35 → 64 → 45 → 65 giây, cách 20 ms: vùng cuối thiếu giọng giữ hình rồi phát tiếp sau **8.828,6 ms**, video tới 65,166 giây. Không còn lỗi vùng cũ hay `play()`/pause. Đây là một phép thử tính đúng, gồm nạp worker/TTS, không phải median tốc độ tua.
- Pause rồi Dừng xử lý: backend `running: false`, không có alert. Đổi nguồn đóng editor thành công. Mở lại qua IPC **212 ms**, UI mở lại đủ 16 câu và nhận đúng 13 WAV hợp lệ.
- Chỉ WAV **014 và 015** được tạo thêm ở vùng cuối; WAV 012, 013 và 016 vẫn thiếu. Cả **11 WAV đầu giữ nguyên SHA-256 và mtime**. MP4 có sẵn trong bản sao không đổi hash/mtime; không tạo MP4 mới, không có `dub.wav` hoặc `.fit.wav`.

Bằng chứng: `acceptance-modes.json`, `acceptance-before.json`, `acceptance-audit.json`, `acceptance-reopen.json`, `acceptance-ui-reopen.json`, `acceptance.png`. Ảnh đã được xem lại; editor hiện đúng 16 câu/13 giọng. Việc tạo WAV chọn lọc và đóng/mở lại này là lượt mới trên EXE cuối, tách biệt với nghiệm thu lịch sử và lượt baseline có lỗi.

Computer Use Windows nhận đúng HWND release nhưng chụp wallpaper và báo `failed to activate captured window` sau khi chọn lại rồi thử một lần. Dừng dùng input Windows, chuyển sang CDP; ảnh `baseline.png` lấy từ Page.captureScreenshot xác nhận editor có video xanh, đủ số câu, timeline và nút xuất. Đây không phải nghiệm thu click/tua bằng chuột Windows bản địa. Không bỏ qua UI: DOM, media và IPC chạy trong WebView Tauri thật.

RAM baseline: 180 mẫu khoảng một giây, gồm cây tiến trình EXE/WebView con, giai đoạn mở nháp dài và TTS. Peak tổng working set **1.159.548.928 byte**, private bytes **2.114.404.352 byte**. Tổng working set có thể đếm trùng trang dùng chung; private bytes không phải resident RAM. Không đo VRAM, không khẳng định mức tối đa hay memory leak từ chuỗi mẫu ngắn. Script dùng Get-CimInstance/Get-Process; không phải ETW.

Bản tối ưu WAV có 180 mẫu trong giai đoạn nháp dài rồi TTS/xuất từ nháp thiếu: peak working set **2.362.642.432 byte**, private bytes **4.262.404.096 byte**. Khối lượng TTS/xuất khác lượt baseline; không dùng hai peak này làm so sánh hiệu quả RAM. Chưa thấy bằng chứng tối ưu giảm peak bộ nhớ.

`npm run verify:local` sau cả hai sửa lỗi phát: cả sáu nhóm đạt; **103 frontend**, **340 Rust đạt, 7 ignored**, hàng đợi/playback/Windows package/build đạt. Bằng chứng cuối `.bench-asr/verify-1793430289308/`; lượt trước `.bench-asr/verify-1793429680475/` có 102 frontend. Benchmark duration release được gọi riêng và đạt; hai benchmark preview cũ có bằng chứng đã chạy trước. Bốn ignored cũ không được coi là đã chạy.

Các test hồi quy hiện có kiểm tra nguồn đổi/receipt cũ/revision cũ, WAV hỏng, đổi mốc giữ WAV, preview và export dùng cùng placement, export thiếu cache, owner và kết quả đến muộn. Đó là kiểm chứng tự động, không phải mới chạy mọi thao tác đa cửa sổ Windows. Chưa có số đo chất lượng giọng/dịch bằng người, bitrate nguồn cao, cold disk hoặc đầy đủ ma trận dịch vụ.

Thử tạo cửa sổ WebView thứ hai bằng IPC thật bị ACL của bản phát hành từ chối: `Command plugin:webview|create_webview_window not allowed by ACL` (`multiwindow-attempt.json`). Không nới quyền để vượt giới hạn này. Quyền owner/late response đa cửa sổ được kiểm chứng ở test backend, chưa nghiệm thu hai cửa sổ GUI đồng thời.

## Tái lập

Hai lệnh Cargo ở trên tạo fixture tạm riêng và tự dọn. Profile release bật opt-level 3, LTO, một codegen unit như Cargo.toml; build lần đầu mất nhiều phút. Không so thời gian build với thời gian chạy benchmark.

Harness UI cục bộ: `.bench-asr/preview-release-20261031/fixture.py`, `cdp.mjs`, `measure.js`, `long-region.js`, `functional.js`, `modes.js`, `ram.ps1`, `audit.py`; raw JSON/PNG nằm cạnh. Fixture generator hiện dựa trên bản sao fixture test đã có trên máy, nên cần nguồn template đó nếu tái lập nguyên xi; benchmark Cargo không có phụ thuộc này. Chỉ trỏ harness vào fixture riêng, không trỏ vào việc thật. Giữ cửa sổ mở khi chạy TTS. Không mở remote debugging cho bản dùng hằng ngày; biến môi trường chỉ đặt trong shell thử.

Chưa commit. Giữ các thay đổi có sẵn của README, DraftStudio, Studio.css và composer.
