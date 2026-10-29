# Nghiệm thu xử lý khi cần — 29/10/2026

Dữ liệu thử riêng ở `output/kiem-thu-khi-can-20261029/`; sự kiện, SHA-256, mốc file, audit FFprobe/giải mã và driver thử ở `.bench-asr/demand-20261029/` (Git bỏ qua). Không dùng lại kết quả thử để đổi file của việc người dùng.

## Cách đo

Chạy cửa sổ Tauri thật, giữ bridge/worker giọng thật. Driver tạm gọi các IPC dùng bởi giao diện, không mock Whisper, TTS, dịch vụ dịch hoặc FFmpeg trong lượt đo. Không gắn driver vào bản sản phẩm cuối.

Nguồn cố định 75 giây: bản cắt Hometown đã có trên máy, chép thành 16 câu. Đường ASR GPU và giọng `ngochuyennew` hiện có; không tải model/giọng. Phép đo chữ đầu dùng DeepSeek local thật, không lấy đó làm benchmark ổn định của dịch vụ. Để so WAV, sửa câu và xuất, cố định lời Việt từ fixture dịch trước; không trộn biến thiên model vào thời gian TTS/dựng. Không có thời gian chờ người dùng trong số đo; driver xác nhận tên cho fixture.

| Mốc | Thời gian quan sát | Điều kiện |
| --- | ---: | --- |
| Mở nguồn local | 1,29 giây | Chưa ASR/dịch/TTS |
| Hình đầu đã giải mã | 1,34 giây từ mở nguồn | File local, WebView thật |
| ASR/ngắt câu | 11,54 giây | Chưa có transcript ở folder thử |
| Chữ dịch đầu | 3,76 giây từ yêu cầu vùng | Đã có ASR; dịch vụ đang sẵn sàng |
| Giọng đầu | 7,07 giây từ yêu cầu nghe | Worker/giọng lần đầu trong cửa sổ; lời cố định |
| Hoàn tất vùng nghe 12 giây + ngữ cảnh | 11,38 giây | Chỉ 4/16 WAV |
| Mở lại nháp | 0,23 giây | Cache ấm |
| Lưu sửa một câu | 0,45 giây | Chỉ ghi nháp |
| Nghe lại câu sửa | 1,87 giây | Worker ấm, tạo lại một WAV |
| Dựng thử 8 giây | 5,64 giây | Chỉ vùng thử + ngữ cảnh có giọng |
| Xuất từ nháp có 4/16 WAV | 60,75 giây | Đủ lời cố định; gồm duyệt tên và xuất có chữ |
| Xuất chỉ phụ đề + Không dịch | 38,21 giây | Nguồn 75 giây; không đọc lại WAV |
| Xuất audio | 8,71 giây | Nguồn 75 giây, lời cố định, cache một phần |

Đây là số đo một máy/lượt, không phải cam kết tốc độ. Thời gian mở/Whisper không tính là preview vùng của ASR: Whisper vẫn một lượt toàn nguồn. Chọn phần đệm 30 giây để có dư so với tốc độ chuẩn bị câu quan sát được và giữ giới hạn công việc; backend vẫn chặn yêu cầu hơn 60 giây.

## Ranh giới đã kiểm chứng

- Chỉ xem chữ vùng đầu dịch 4/16 câu, không tạo WAV.
- Chỉ nghe vùng đầu tạo 4/16 WAV; không có `dub.wav` hay MP4 trước khi bấm Xuất.
- SHA-256 và mốc file chứng minh lưu sửa giữ cả bốn WAV; nghe lại chỉ đổi WAV câu sửa. Xuất tiếp giữ cả bốn WAV còn hợp lệ. Đổi mốc sau xuất giữ nguyên cả 16 WAV và bản xuất trước.
- Video phụ đề không thay bất kỳ WAV nào. Audit đầu giải mã được cả chín file media của fixture; không còn `audio.wav`, `dub.wav`, `.fit.wav` hoặc `.partial.mp4` sau khi xong.
- Dừng phát sau giọng đầu: chỉ hai WAV kịp hoàn tất, không chạy tới hết 16 câu. Tua về cuối chỉ tạo câu 13–16 cùng phần đã có; không đọc phần giữa. Hủy ngay khi TTS nhận việc trả `Đã dừng.`, giữ sáu WAV trước đó và nhả quyền sau khi worker trả về. Chạy tiếp vùng giữa thành công.
- Nguồn 17 giây có 4 câu, mới dịch 1 câu: xuất đã duyệt tên London → Luân Đôn, áp dụng thuật ngữ, hoàn tất đủ 4 lời/4 WAV và MP4 có chữ. Lượt đạt sau xác nhận tên: 15,89 giây từ yêu cầu xuất. Một lượt driver trước đó dùng nhầm `keep: true` (giữ lời gốc), nên phép kiểm tên bị từ chối; đã sửa driver và chạy lại trên nguồn mới.
- Nguồn chưa có ASR: xuất thẳng ở chế độ phụ đề + Không dịch hoàn tất 4 câu, không tạo WAV. Chọn API chưa lưu key không khóa nhầm lượt này.
- Lưu từ textarea của giao diện thật đã ghi đúng lời, không phát `tts-request`, không đổi SHA-256 MP4. Nút Xuất nằm trong viewport 1080×760 (mép dưới 743,83px).
- Đổi riêng giọng của fixture sang `banmai`: lời không đổi, WAV cũ không còn được coi sẵn sàng, nghe một câu chỉ tạo một WAV và không gọi dịch. Đổi ngôn ngữ sang `en` trong chế độ phụ đề xóa lời phụ thuộc, giữ đủ 16 ID. Đã phục hồi cấu hình/lời/giọng fixture và xác nhận lựa chọn giọng toàn app không đổi.
- Sau xuất có thuật ngữ, mở lại preview giữ nguyên lời và WAV, không gọi dịch/TTS. `timeline-comparison.json` đối chiếu mốc/độ dài cả 4 câu với `voice-spans.json` thật của bản xuất: sai khác lớn nhất 0 giây. Đây là lịch video; audio xuất liền mạch theo hợp đồng audio hiện có.
- WAV phục vụ phát mang SHA-256 trong URL, để trình phát không giữ tiếng cũ khi cùng đường dẫn vừa được đọc lại. Test kiểm tra câu tạo lại đổi version, câu không sửa giữ version; WebView kiểm tra tải media có version riêng.
- Hàng đợi thật gọi `jobActions.run` cho hai video tiếng Việt 8 giây. Thêm B với khung 9:16 trong lúc A chạy, rồi đổi cấu hình đang soạn sang 1:1: cả A/B hoàn tất, giải mã được, giữ 9:16, hàng chờ rỗng. Không dịch nên không gọi AI tên/thuật ngữ/dịch. Đã trả lựa chọn đang soạn và bỏ riêng hai dòng lịch sử fixture khỏi giao diện; sự kiện kiểm thử vẫn giữ trong biên bản thô.
- Unit/integration tests kiểm tra ghi atomic, phản hồi cũ, receipt/source/model, giữ ID/slot, hoàn tác, journal cấu hình, phạm vi vô hiệu hóa, bounded queue, owner cửa sổ và dùng cùng lịch preview/đoạn thử/xuất.

## Kiểm tra mã cuối

- `npm run verify:local`: tất cả sáu nhóm đạt. Report `.bench-asr/verify-1793271726581/report.json`: 100 bài frontend, 339 bài Rust đạt; bốn bài benchmark/probe được đánh dấu bỏ qua. Có thêm harness hàng đợi, playback, kiểm tra đóng gói Windows và build.
- Bốn bài bỏ qua là benchmark encoder/fit, probe Ollama và đo song song tool checks; không ghi chúng là bài đã chạy. Chức năng có test riêng và các lượt Tauri/FFmpeg thật nêu trên. Fallback NVENC → CPU có test FFmpeg thật và kiểm tra giải mã.
- Sau khi tháo hook hàng đợi cuối, `npm run build` lại thành công, 100 module; `src/main.tsx` không đổi so với trước thử. `git diff --check` không lỗi; tìm trong `src`/Rust không còn driver hay URL collector 1429.
- Audit chín file media của phép đo chính và tám file bổ sung đều giải mã không lỗi. Hai kết quả hàng đợi cũng đạt FFprobe/giải mã và đúng tỉ lệ 9:16. Xem `audit.json`, `additional-media.json`, `queue-media.json`, `timeline-comparison.json` trong thư mục bằng chứng.
- Không commit. Không đổi tên app, identifier, runtime Tauri, cơ chế tải hoặc thêm model/giọng/công cụ.

## Đối chiếu kế hoạch

| Mục | Bằng chứng |
| --- | --- |
| 1.1–1.3 Nháp/version/revision, commit theo lô, ID/cache | `draft.rs`, `text_cache.rs`, `atomic_file.rs`, `editing.rs`; tests receipt/hash/source/late response/undo; dừng/chạy tiếp thật và audit WAV |
| 1.4–1.5 Trạng thái chuẩn bị/xuất và quyền sở hữu | Coordinator lease, phase, marker xuất; bounded queue; hủy TTS thật chỉ nhả quyền sau worker, chạy tiếp thành công |
| 1.6 Việc cũ và phục hồi | Legacy ID/slot, nguồn/canonical giữ nguyên, báo cache chưa xác minh; tests import và journal bị ngắt; lịch sử bản xuất giữ được |
| 2.1–2.4 Lưu/nghe/xuất riêng | Lưu UI không TTS/MP4, nghe chỉ thay một WAV, đổi mốc giữ WAV, cấu hình không AI/TTS, editor mở từ nguồn không đòi MP4 |
| 3.1–3.3 Vùng nạp trước/tua/dừng | Giới hạn 30/60 giây, unit queue một chạy + một chờ mới nhất; xem chữ 4/16 câu không WAV; dừng/tua/hủy thật |
| 3.4–3.5 Lịch video, tên/thuật ngữ | Cùng `place_all` và WAV hợp lệ cho preview/đoạn thử; cảnh báo ngữ cảnh; mốc thật sai khác 0; sửa tên/thuật ngữ trước xuất và cache chính thức dùng lại khi xem |
| 4.1–4.3 Xuất từ bất kỳ nháp nào | Xuất nguồn chưa ASR; xuất mới dịch 1/4 câu và mới có 4/16 WAV; chế độ phụ đề/Không dịch/audio; cấu hình và sửa bị khóa trong lượt xuất |
| 4.4–4.5 Đường xuất và Chạy toàn bộ | FFmpeg/NVENC/CPU, partial/lịch sử, media giải mã; hàng đợi thật hai video giữ cấu hình snapshot; UI và hướng dẫn cập nhật |
| 5 Kiểm chứng và đo | Các bảng thời gian, nguồn và giới hạn; audit hash/mốc/giải mã, biến thể giọng/ngôn ngữ, hồi quy local và queue thật ở trên |

## Nguồn link

| Nguồn | Bằng chứng hiện tại |
| --- | --- |
| YouTube video | `open_draft` tải/mở nguồn Hometown bằng đường yt-dlp hiện có; chưa dịch/TTS. Lượt mở cache 1,75 giây. Không dùng số này làm thời gian tải lạnh. |
| YouTube Audio có phụ đề | `open_draft` nhận transcript qua `prepare_listen`, không tải hình (19,01 giây). Cues trong editor chỉ công bố sau thao tác lấy lời. |
| Douyin | Link `IrX0XVPXjUY` tải qua helper Edge/phiên khách/ký request hiện có, mở media thành công (81,21 giây), không chạy yt-dlp hoặc pipeline sau tải. |
| TikTok | Mẫu đầu trả `Your IP address is blocked from accessing this post` sau 4,02 giây. Mẫu khác từ test chính thức yt-dlp (`@patroxofficial/video/6742501081818877190`) tải qua `open_draft`, giải mã trong WebView thành công: 27,47 giây media, 5,87 giây tới hình đã đọc được; chưa có ASR/WAV. Không đổi cơ chế tải, không lấy cookie hay vượt chặn. |
| Phụ đề không hợp lệ/không có | Tests `source_captions` giữ ASR fallback, từ chối track tự dịch/không rõ ngôn ngữ và timeline hỏng; giữ cache ASR đã có. Nguồn local 75 giây thực tế xác nhận đường không phụ đề → Whisper. |

Lượt driver ban đầu bị reload trong khi IPC tải đang chạy; lần gọi chồng bị bộ điều phối từ chối đúng `Đang xử lý một yêu cầu khác.`. Đã đợi quyền rồi chạy lại tuần tự. Đã bổ sung gắn URL cho download mới sau khi xác nhận media, và từ chối folder nghe của nguồn khác; dữ liệu cũ không bị gán nhầm.

## Giới hạn

- Các nền tảng có thể từ chối một video cụ thể; mẫu TikTok bị chặn được ghi là lỗi, không thay bằng kết luận mọi link đều tải được.
- Chưa đánh giá chất lượng phát âm bằng người, nhiều người nói hay mọi dịch vụ API/ngôn ngữ. Không dùng kiểm tra kỹ thuật để khẳng định chất lượng dịch/giọng.
