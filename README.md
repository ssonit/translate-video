# Lồng tiếng video trên máy

App desktop để chọn một video trên máy, tách tiếng, chép lời, dịch, đọc lại bằng giọng tiếng Việt, rồi ghi video mới vào folder local. Không gửi audio hay video đi. Tên app chưa chốt.

Mở **Hướng dẫn** trên thanh điều hướng trong app để xem cách chuẩn bị máy, lồng video đầu tiên, sửa lời và xuất kết quả.

Muốn xem video kèm phụ đề dịch mà không có giọng đọc mới: chọn **Mở nguồn & sửa nháp** → **Phụ đề (không đọc)** → **Phát**. Chế độ này giữ tiếng gốc, chỉ dịch phần đang xem và không tạo hay phát giọng lồng; không cần xuất video để xem.

**Mở nguồn & sửa nháp** mở video gốc trước; **Lấy lời gốc**, **Bản dịch** và **Giọng lồng** chỉ chạy phần bạn yêu cầu. Dịch/giọng nạp trước khoảng 30 giây, tua ưu tiên vùng mới. **Lưu nháp** không dựng MP4; **Nghe câu** tạo lại câu bị sửa; **Xuất video/audio** hoàn tất phần thiếu và xuất bất kỳ lúc nào. **Chạy toàn bộ** và hàng đợi vẫn xử lý thẳng cả nguồn. [Chi tiết triển khai](docs/trien-khai-khi-can-dot-4-2026-10-29.md) · [Kết quả kiểm chứng và giới hạn](docs/nghiem-thu-khi-can-2026-10-29.md).

## Luồng

1. FFmpeg tách tiếng.
2. Whisper chép lời.
3. Dịch. Có API key Gemini, ChatGPT, Claude hoặc DeepSeek thì dùng luôn (chỉ chữ lời thoại đi qua mạng). DeepSeek local thì bấm Bật DeepSeek, app gọi server Deepseek-API trên máy tại `http://127.0.0.1:8000` (model `deepseek-chat` hoặc `deepseek-expert`), không cần key. Ollama không bắt buộc, chỉ cài nếu muốn dịch trên máy: model `qwen3.5:9b-q4_K_M` hoặc `gemma3:12b` tại `http://localhost:11434`.
4. Giọng Piper đọc tiếng Việt bằng file giọng trong `voices/vi`. Có `piper.exe` trong folder `piper` thì app đọc bằng chương trình đó. Không có thì giọng chạy ngay trong cửa sổ app (ONNX Runtime Web).
5. FFmpeg ghép video, phụ đề và tiếng lồng vào folder người dùng chọn.

## Hiện có

- Giao diện chia Lồng tiếng, Hàng đợi, Thư viện, Bộ cài đặt và Cài đặt; thư viện không còn chiếm cột bên cạnh mọi màn. Hình/chữ, âm thanh và đoạn thử có trang cấu hình riêng. Màn chỉnh sửa có timeline toàn chiều ngang, kéo chia vùng video/lời thoại và thanh xuất giữ trạng thái nháp. Theme than trung tính/cam học cách phân vai màu của Folia. Phạm vi triển khai và kiểm chứng: [Giao diện mới](docs/giao-dien-moi-2026-10-28.md).
- Mở app là vào thẳng màn lồng tiếng, việc kiểm tra máy chạy ở nền. Kết quả là chip ở góc phải: "Đủ đồ nghề" hoặc "Thiếu Whisper, xem". Thiếu mục bắt buộc thì các nút Chạy bị khóa, kèm dòng báo thiếu gì.
- Màn chuẩn bị (bấm chip để mở): kiểm tra FFmpeg, Ollama (chỉ cần khi dịch trên máy), model dịch, giọng, Whisper. Mục thiếu thì hiện cách tải. App không tự tải gì, trừ nút "Tải Whisper".
- `voices/<mã ngôn ngữ>` (`vi`, `en`, `zh`, `ja`, `ko`, `fr`, `de`, `es`, `ru`, `th`, `id`, `pt`): các cặp `.onnx` và `.onnx.json`. Giọng được đọc bằng `onnxruntime-web` và `phonemizer` (espeak-ng WASM) trong Web Worker, kèm bộ chuẩn hóa tiếng Việt (số, ngày, tiền, từ viết tắt) lấy từ project Local-TTS (`src/tts`). Một luồng, khoảng 1,8 lần thời gian thực.
- Chọn giọng và nghe thử: mục "Giọng Việt" trên màn chuẩn bị có ô chọn giọng (lưu lại cho lần sau) và nút Thử giọng đọc một câu mẫu.
- Khi đọc giọng, Rust gửi danh sách câu cho cửa sổ app, cửa sổ đọc rồi gửi wav về để ghi vào folder việc. Phải giữ cửa sổ app mở khi đang lồng.
- Màn lồng tiếng: chọn video và folder kết quả, bấm Lồng tiếng hoặc Dừng, xem trạng thái từng bước.
- Chọn một loại kết quả: **Video lồng tiếng**, **Audio** hoặc **Video phụ đề**. Chỉ video lồng tiếng mới hiện tùy chọn kèm phụ đề; video phụ đề không hiện chọn giọng. Màn chính tóm tắt loại kết quả, ngôn ngữ, giọng và nơi lưu trước khi chạy.
- Mở **Khung hình & kiểu chữ** trên màn lồng tiếng để chọn khung gốc, 9:16, 1:1 hoặc 16:9. Khung mới có hai cách đặt hình: giữ toàn hình với nền mờ, hoặc cắt đầy khung và kéo hình để giữ nhân vật. Cạnh ngắn bản đổi khung tối đa 1080 px, không tăng từ nguồn có cạnh ngắn nhỏ hơn; kích thước làm tròn số chẵn để mã hóa video.
- Bật **Kèm phụ đề trên video** hoặc chọn **Video phụ đề** để chỉnh font, cỡ chữ, màu, viền, nền và kéo vị trí chữ. Có chữ mẫu, chọn mốc hình xem thử và vùng gợi ý tránh giao diện video ngắn; vùng gợi ý không ghi vào video. Mẫu bố cục trong cửa sổ có thể xuống dòng hơi khác FFmpeg. Xem trước dịch trực tiếp vẫn phát khung gốc; lựa chọn hình và chữ áp dụng lúc xuất.
- **Lưu bộ mới** ghi bộ cài đặt theo kênh trên máy: khung hình, kiểu chữ, ngôn ngữ, giọng, loại kết quả và tùy chọn phụ đề/không dịch/thuật ngữ; không lưu API key hay đổi dịch vụ dịch. Các bộ có thể chọn lại và xóa trong **Quản lý bộ đã lưu**. Mỗi video trong hàng đợi giữ riêng bố cục lúc thêm. Thay khung hình rồi chạy lại cùng video sẽ cất bản xuất cũ vào `cu/`, giữ lời chép, bản dịch và WAV từng câu. Đổi riêng kiểu chữ chỉ cất bản có chữ cũ và ghi lại phụ đề; giữ nguyên video chưa có chữ để không ghép lại khung. Folder việc lưu bố cục ở `render.json`; việc cũ không có file này dùng mặc định.
- Mỗi video trong hàng đợi giữ dịch vụ, model và tùy chọn thuật ngữ lúc thêm, kể cả khi đổi dịch vụ cho video sau hoặc mở lại app. Key lấy từ nơi lưu key của dịch vụ đó, không lưu trong hàng đợi. Nút **Dùng lựa chọn hiện tại** cập nhật riêng video đang chờ. Hàng cũ chưa có cấu hình dịch sẽ dùng lựa chọn hiện tại lúc bắt đầu, rồi giữ cấu hình đó cho lần thử lại.
- Tiến độ mặc định gồm bốn giai đoạn, có thể mở chi tiết chín bước. Khi chờ kiểm tra tên, trạng thái hiện cả trong danh sách video; thời gian chờ không tính vào thời gian xử lý, ước tính còn lại hay `timing.log`.
- Xem trước chưa áp dụng sửa tên, thuật ngữ và co độ dài như khi xuất. App ghi rõ khác biệt này bên cạnh thao tác xem trước.
- Khung xem bám đúng tỉ lệ thực tế của video, kể cả sau khi xoay. Màn chỉnh sửa đặt video và lời thoại cạnh nhau khi đủ rộng, có vạch kéo chia diện tích; khung hẹp xếp lời thoại bên dưới. Tốc độ, tải video và hình trong hình nằm chung trong menu ba chấm ở góc trên của video; nút tự ẩn khi đang phát và chuột rời hình.
- Khi chuyển video, màn giữ trạng thái "Đang mở video…" đến khi kiểm tra mở file, đọc tỉ lệ hình và lấy lời thoại cùng danh sách bản lưu xong, rồi hiện bố cục một lần. Kết quả tải chậm của video đã bỏ qua không thay nội dung đang xem; lỗi đọc file hiện thông báo thay vì giữ màn tải.
- Khi ghi chữ lên video, app giữ khung hình gốc, tính cỡ chữ theo cạnh ngắn và chừa lề theo tỉ lệ hình. Câu dài hiện lần lượt từng đoạn tối đa hai dòng trong khoảng thời gian của câu, chia thời gian theo độ dài chữ (không phải căn từng từ theo giọng). File SRT, lời dịch và WAV từng câu vẫn giữ nguyên; bố cục mới áp dụng khi xuất bản có chữ mới.
- Danh sách lời thoại đánh dấu câu chưa có lời, thiếu file tiếng từng câu, bị cắt và mức tăng tốc; nút **Đến câu cần kiểm tra** duyệt lần lượt. Bấm **Sửa** để bổ sung hoặc rút gọn rồi tạo lại các câu đã sửa. Thiếu file tiếng chỉ nói về WAV từng câu để tạo lại, không khẳng định bản xuất bị mất tiếng. Báo cáo cũ vẫn hiện phần bị cắt; mức tăng tốc chỉ có ở bản ghép mới. Audio liền mạch và chế độ chỉ chữ không hiện cảnh báo co giọng; cả hai vẫn báo câu chưa có lời, và không kiểm tra WAV từng câu ở audio liền mạch hay chế độ chỉ chữ.
- Pipeline (`src-tauri/src/pipeline`): tách tiếng, chép lời, ngắt câu, sửa tên (lần đầu dừng để sửa tên bị chép sai; phụ đề và giọng dùng tên đó), lấy thuật ngữ, dịch (theo cụm 10 câu, có ngữ cảnh), đọc giọng (đọc cả lô câu một lần nạp giọng; câu quá dài được rút gọn rồi đọc lại), co độ dài (xếp câu nối tiếp, mượn khoảng trống tối đa 1,5 giây, tăng tốc tối đa 1,4 lần, câu vẫn không vừa thì cắt), ghép video.
Mỗi video có một folder việc trong folder kết quả, đặt theo tên video:

- `audio.wav`, `transcript.json`, `lines.json`, `names.json`, `terms.json`, `translation.json`
- `lines/001.wav`, `lines/002.wav`, ... tiếng đọc từng câu; `lines/001.fit.wav`, ... bản đã co độ dài
- `dub.wav` tiếng lồng cả video; `fit-report.json` liệt kê câu bị cắt bớt
- `<tên video>.vi.mp4` và `<tên video>.vi.srt` kết quả

Folder kết quả mặc định là `output/` ở gốc project (app tự tạo, đổi được bằng nút "Đổi folder"). Ví dụ `output/tap 1/tap 1.vi.mp4`.

Lời dịch và thuật ngữ được dùng lại khi dấu xác nhận còn khớp nguồn, cấu hình và nội dung đã lưu; WAV từng câu cũng được đối chiếu lời đọc/giọng và SHA-256 khi cần dựng lại. Bản dịch lưu sau từng cụm nên dừng giữa chừng vẫn dịch tiếp phần thiếu. Cache cũ chưa có dấu xác nhận có thể được cất vào `cache-stale/` rồi làm lại một lần; bản xuất cũ giữ trong lịch sử. Chi tiết: [nháp và xác minh cache](docs/trien-khai-khi-can-dot-3-2026-10-29.md).

Mỗi thời điểm chỉ một tác vụ xử lý giữ quyền ghi. Bấm **Dừng** ngừng phát preview ngay và yêu cầu hủy; nếu đang gọi dịch vụ AI, app giữ trạng thái đang dừng đến khi HTTP hiện tại kết thúc hoặc timeout, rồi mới cho chạy việc kế tiếp. Không còn để request dịch chạy ngầm sau khi đã báo dừng.

Whisper: mục Whisper ở màn chuẩn bị có nút **Tải Whisper**. Một cú bấm: app kiểm tra bản whisper.cpp mới nhất trên GitHub và model trên Hugging Face, chỉ tải phần còn thiếu hoặc cũ hơn (khoảng 475 MB lần đầu), đối chiếu mã băm SHA-256 rồi tự giải nén vào folder `whisper` trong project. Đã đủ và mới nhất thì không tải lại. Tải tay vẫn được: giải nén `whisper-bin-x64.zip` vào folder `whisper` và chép `ggml-small.bin` (hoặc base, medium, large-v3, tiny) vào cùng folder. Nếu `whisper-cli` đã có trong PATH thì dùng luôn, model nằm cạnh nó. Màn chuẩn bị báo rõ thiếu file nào.

Đã kiểm tra bước căn tiếng và ghép bằng WAV/video thật; kết quả benchmark nằm ở `docs/workflow-review.md`. Ngày 28/10/2026 đã chạy trọn luồng với nguồn local 75 giây qua Whisper → DeepSeek local → Piper → video 9:16 có chữ, kiểm tra dừng/chạy tiếp, đổi kiểu chữ và hàng đợi hai video. Chi tiết và giới hạn ở `docs/e2e-review-2026-10-28.md`; chưa xác nhận trọn luồng qua mọi dịch vụ dịch.

## Giảm thời gian xử lý

Xuất video cần mã hóa hình ưu tiên NVENC trên máy có NVIDIA, tự dùng lại CPU nếu GPU lỗi; ghép tiếng không đổi hình vẫn copy video. Trên video Douyin Full HD 6 phút, phép đo cùng hình/phụ đề giảm phần ghi chữ từ 128,84 xuống 29,70 giây, file lớn hơn khoảng 14%. Bản thử đã sửa riêng bốn câu bị cắt bằng rút lời/chỉnh mốc, báo cáo căn tiếng còn 0 câu bị cắt; [số đo, cách sửa và giới hạn](docs/gpu-va-bon-cau-2026-10-29.md).

Whisper GPU dùng lọc vùng có giọng nói và tránh kéo lỗi nhận dạng từ đoạn trước. Việc mới ghép các mẩu câu sát nhau trước khi dịch; bản dịch lồng video nhận thời lượng từng câu để viết gọn ngay từ đầu. Audio liền mạch giữ đủ lời, không rút gọn theo mốc hình. Câu ngắn bị đứng trong bộ đọc của cửa sổ được thử lại một lần với worker mới. Việc cũ giữ cache; dùng Chép lời lại nếu muốn chia câu theo cách mới. Trên video Douyin đã thử, câu bị cắt giảm từ 41 xuống 4; [kết quả và giới hạn](docs/toi-uu-loi-va-giong-2026-10-29.md).

Video tải mới bằng yt-dlp giữ thêm metadata cạnh file nguồn. Nếu có phụ đề do kênh cung cấp khớp ngôn ngữ gốc và mốc/nội dung phù hợp, app lấy phụ đề đó vào `transcript.json`, bỏ qua tách tiếng và Whisper. Xem trước và xuất dùng chung cache này. Thiếu metadata, không rõ ngôn ngữ gốc, chỉ có phụ đề tự động, phụ đề không phù hợp hoặc tải phụ đề lỗi thì vẫn dùng Whisper. Không ghi đè lời chép hay lời đã biên tập của việc cũ; file local và video Douyin vẫn theo đường hiện tại.

Khi đọc giọng trong cửa sổ app, sau bước duyệt tên/thuật ngữ, câu dịch đã lưu được đưa sang một tác vụ đọc giọng trong lúc lô tiếp theo còn đang dịch. Hàng chờ giới hạn hai lô; giọng giữ nguyên trong worker, mỗi câu chỉ đưa sang một lần. Câu còn chờ dịch lại chưa được tạo WAV im lặng. Lỗi/hủy dừng cả hai tác vụ và giữ phần đã lưu để chạy tiếp. Piper native vẫn nhận cả lô trong một tiến trình để tránh nạp lại model. `timing.log` ghi thời gian chồng nhau ở mục `translate+speak`; bước đọc giọng sau đó vẫn kiểm tra/rút gọn câu dài như trước. Đã có [số đo thử trên video Douyin](docs/thu-toc-do-douyin-2026-10-29.md), kèm giới hạn do câu trả lời model thay đổi; xem thêm [phạm vi kiểm chứng](docs/tang-toc-xu-ly-2026-10-29.md).

## Chạy

```powershell
npm install
npm run tauri dev
```

`npm run tauri dev` mở cửa sổ app. `npm run build` chỉ build frontend. `npm run tauri build` build desktop và bộ cài khi máy đã có công cụ đóng gói. Build EXE bằng dependency hiện có: `npm run tauri build -- --no-bundle -- --offline`. Hướng dẫn tài nguyên và bản chạy thử: [Windows release](docs/windows-release.md).

Kiểm tra hồi quy trên Windows bằng `npm run verify:local`: frontend, hàng đợi, kiểm tra gói, build frontend và Rust offline; log và báo cáo nằm trong `.bench-asr/verify-<mốc thời gian>/`. Chi tiết lượt bổ sung: [Kiểm thử phát hành](docs/kiem-thu-phat-hanh-2026-10-28.md).

## Nâng cấp xuất video ngày 28/10/2026

Trong **Khung hình & kiểu chữ** có các phần mở rộng: trộn tiếng gốc/tiếng lồng và tự hạ tiếng gốc theo khoảng giọng thực tế; bảng cách đọc tên theo ngôn ngữ chỉ áp dụng khi đọc; logo PNG/JPEG; vùng che màu/làm mờ chữ nguồn; dựng đoạn thử 8 giây bằng FFmpeg từ cache. Bộ cài đặt và hàng đợi giữ các lựa chọn đó. Đoạn thử cần bản dịch và WAV đã tạo, không chép lời hay gọi AI lại. Với kết quả hiện tại, **Sửa bản xuất** nạp cấu hình để chỉnh rồi xuất lại.

Thay âm lượng giữ tiếng từng câu và có thể dùng lại video đã dựng; thay cách đọc chỉ xóa WAV của các câu bị ảnh hưởng. Logo được lưu bằng SHA-256 và chép riêng vào folder việc để việc cũ không lệ thuộc ảnh ngoài. Che chữ thực hiện trên hình nguồn trước đổi khung, logo đặt sau đổi khung, chữ Việt đặt cuối. Không có OCR hay phục hồi nền bằng AI.

Hàng đợi có chọn nhiều việc chờ/áp bộ cài đặt, đổi thứ tự, bỏ qua việc đang chạy, lịch sử thử lại và chính sách dừng/tiếp tục khi lỗi. Mặc định dừng khi lỗi. Chờ duyệt tên vẫn cần người dùng xác nhận; lịch sử không lưu key. Báo cáo triển khai và giới hạn: [Nâng cấp](docs/nang-cap-2026-10-28.md).

Phụ đề ghi lên video chia theo cụm ký tự Unicode và kiểm tra giữ đủ nội dung. Mốc sai hoặc thời gian quá ngắn để hiện đủ đoạn báo lỗi rõ thay vì bỏ chữ; giữ nguyên SRT, bản dịch và WAV. Danh sách lời thoại của video có chữ đánh dấu mốc chồng nhau, câu hiện quá nhanh hoặc chữ dày để xem lại; ngưỡng đọc chỉ là gợi ý. Chi tiết kiểm chứng và giới hạn: [Phụ đề an toàn](docs/phu-de-an-toan-2026-10-28.md).

Video kết quả có **Chỉnh thời gian câu**: nhập mốc hoặc kéo đầu/cuối/thân câu trên sóng âm, nghe lặp bản đang xem, hoàn tác và xuất lại. Đổi riêng mốc giữ WAV, không gọi lại Whisper/dịch/TTS; mốc sửa lưu riêng ở `timing.json`. Nháp chưa thay bản đang phát, chỉ áp dụng sau **Xuất lại**. Audio liền mạch không dùng phần chỉnh mốc này. Kiểm chứng và giới hạn: [Chỉnh mốc câu](docs/chinh-moc-cau-2026-10-28.md).

Trong phần chỉnh thời gian có **Tách tại vị trí phát**, **Gộp với câu sau** và **Hoàn tác tách/gộp**. Xem lại cả lời gốc/lời đọc trước khi xuất; app chỉ đọc tiếng các câu mới, giữ file của câu khác bằng ID ổn định trong `editing.json`. Lời Whisper và bản dịch nền giữ nguyên. Có thể hoàn tác tối đa 20 thao tác qua lần mở lại; câu đã sửa tiếp được bảo vệ khỏi ghi đè. Việc đã tách/gộp chạy tiếp từ cấu trúc đã lưu; Xem trước dịch trực tiếp báo dùng bản kết quả/Xuất lại để tránh nhầm mốc gốc. Chi tiết: [Tách/gộp câu](docs/tach-gop-cau-2026-10-28.md).

Màn kết quả có **Lịch sử & cấu hình**: xem thời điểm, thay đổi, lựa chọn hình/tiếng và mở/tải từng bản video hoặc audio đã xuất. **Nạp cấu hình để sửa tiếp** là thao tác riêng, dùng lời/mốc hiện tại và dịch vụ dịch đang chọn; bản cũ thiếu metadata không được đoán cấu hình. Khi đang chạy, app khóa đổi phiên bản/nạp cấu hình. Lịch sử không lưu API key hoặc sao chép WAV từng câu; các bản lỗi/hủy chưa chốt không được ghi thành bản thành công. Chi tiết: [Lịch sử bản xuất](docs/lich-su-ban-xuat-2026-10-28.md).

Rà soát trước push và gói EXE chạy thử mới nhất: [Báo cáo 28/10/2026](docs/ra-soat-truoc-push-2026-10-28.md). Bộ kiểm tra local gồm cả chọn phiên bản với phản hồi IPC đến sai thứ tự; gói chưa thay thế nghiệm thu installer trên Windows sạch.
