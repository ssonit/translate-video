# Kế hoạch xử lý khi cần — 29/10/2026

Trạng thái: đã hoàn thành triển khai và kiểm chứng năm giai đoạn trong phạm vi kế hoạch. Đã có [lưu an toàn và phiên đọc giọng — đợt 1](trien-khai-khi-can-dot-1-2026-10-29.md), [manifest nháp/revision — đợt 2](trien-khai-khi-can-dot-2-2026-10-29.md), [xác minh cache và điều phối — đợt 3](trien-khai-khi-can-dot-3-2026-10-29.md), [nháp/preview vùng/xuất — đợt 4](trien-khai-khi-can-dot-4-2026-10-29.md). [Biên bản nghiệm thu](nghiem-thu-khi-can-2026-10-29.md) đối chiếu từng nhóm yêu cầu, số đo, kiểm tra local và giới hạn. Đã mở/tải được nguồn local, YouTube, Douyin và TikTok; không suy rộng thành mọi link hay mọi dịch vụ đều đã nghiệm thu.

## Mục tiêu

Người dùng mở nguồn, xem/nghe và sửa bản nháp trước; chỉ tạo file kết quả khi bấm Xuất. Không tự dịch, đọc giọng và dựng hết video chỉ vì đã mở nguồn. Hàng đợi vẫn có chế độ chạy toàn bộ đã được người dùng yêu cầu.

Giữ Tauri, nguồn tải hiện có, giọng hiện có và nguyên tắc không gửi audio/video lên mạng. API riêng chưa thuộc phạm vi triển khai này.

## Căn cứ code hiện tại

- `pipeline/mod.rs::run` vẫn chạy từ chuẩn bị tới xuất, dùng lại cache hợp lệ và có dịch/giọng chồng nhau.
- `watch.rs::run_watch` đã phát trên video gốc nhưng xử lý tuần tự tới hết; `publish` chỉ công bố cache chính sau khi hoàn tất. Dừng/lỗi dọn dữ liệu tạm theo hợp đồng hiện tại.
- `WatchLive.tsx` chỉ hiện nút xuất khi `published`; `Work.tsx` chuyển thao tác đó sang `job.runFrom` và pipeline đầy đủ.
- `pipeline/retake.rs` đã tái sử dụng WAV không đổi, nhưng sửa câu vẫn nối tiếp căn tiếng và dựng lại kết quả.
- Đã có mô phỏng kiểu chữ/khung và đoạn thử FFmpeg. Đoạn thử hiện đòi dữ liệu giọng đã chuẩn bị, cần thu hẹp phụ thuộc về đúng khoảng thử.
- NVENC đã có và được đo riêng: ghi chữ 128,84 xuống 29,70 giây. Lượt xuất lại 80,85 giây dùng cache; không dùng số đó làm thời gian pipeline mới.

## Hành vi đích

| Thao tác | Phần được thực hiện |
| --- | --- |
| Dán link | Nhận diện/chuẩn hóa link; chưa khởi động pipeline |
| Mở nguồn | Tải nguồn theo đường hiện có nếu cần, đọc metadata, phát video gốc; mở lại nháp nếu có |
| Xem lời gốc | Lấy phụ đề hợp lệ; nếu thiếu, tách audio và chạy Whisper một lượt cho nguồn |
| Xem bản dịch | Dịch vùng yêu cầu và phần đệm phía trước; không tạo giọng nếu chưa yêu cầu nghe |
| Nghe bản lồng | Đảm bảo vùng đó có lời dịch, tạo WAV cần thiết, chuẩn bị căn tiếng cục bộ và phát |
| Sửa lời/mốc | Lưu nháp; đánh dấu đúng dữ liệu phụ thuộc đã cũ; chưa dựng MP4 |
| Nghe câu đã sửa | Tạo lại giọng câu bị ảnh hưởng; cập nhật lịch phát liên quan |
| Đổi hình/chữ/logo | Mô phỏng trong cửa sổ; chưa dựng toàn bộ video |
| Dựng đoạn thử | Chuẩn bị và dựng đúng khoảng yêu cầu cùng ngữ cảnh căn tiếng cần thiết |
| Xuất video/audio | Chốt cấu hình, hoàn tất phần thiếu, căn toàn bộ, trộn và xuất |
| Chạy toàn bộ/hàng đợi | Thực hiện trọn luồng theo yêu cầu đã chọn |

Không hứa phát link ngay trước khi tải xong, hoặc Whisper chép riêng vùng tua. Giai đoạn đầu giữ cách tải hiện tại và một lượt Whisper toàn nguồn; kết quả ASR có thể xuất hiện dần. Dịch và TTS mới là các tác vụ được giới hạn theo vùng yêu cầu.

## Giai đoạn 1 — Bản nháp bền vững và điều phối tác vụ

1. Bổ sung trạng thái nháp có phiên bản: nguồn, cấu hình, revision, câu đã chép/dịch/đọc và vùng đã sẵn sàng. Không coi file có mặt là đủ điều kiện tái sử dụng.
2. Ghi dữ liệu sau từng cụm thành công bằng file tạm rồi thay thế; WAV chỉ đánh dấu sẵn sàng sau khi ghi hợp lệ. Dừng/đóng app giữ phần đã chốt, dọn phần đang ghi dở.
3. Dùng ID câu ổn định, tái sử dụng cơ chế biên tập hiện có. Cache gắn với nội dung nguồn/lời đọc, ngôn ngữ, giọng và cấu hình ảnh hưởng; chống nhận nhầm kết quả từ revision cũ.
4. Tách trạng thái đang chuẩn bị nháp, đã có vùng nghe và đang xuất; không dùng `published` của preview để khẳng định đã có file xuất.
5. Chỉ một bộ điều phối sở hữu tác vụ của việc đang chạy. Hủy phải chờ tiến trình/worker dừng; giữ giới hạn DeepSeek local một request một lúc và hàng đợi có giới hạn.
6. Đọc được việc cũ; không tự đổi ID/chia lại câu hoặc ghi đè bản đã xuất. Khi chuyển đổi nháp phải có cách phục hồi nếu lỗi. Giữ hợp đồng preview cũ tới khi đường mới được kiểm chứng.

Nghiệm thu: dừng ở giữa dịch hoặc TTS rồi mở lại chỉ làm phần thiếu; đóng app giữa lúc ghi không làm hỏng cache; không phát/ghi response của phiên đã bỏ qua.

## Giai đoạn 2 — Tách chỉnh sửa khỏi xuất

1. Tách lưu chỉnh sửa, chuẩn bị giọng từng câu và xuất thành các tác vụ riêng; tái sử dụng validation, undo và ID của `editing.rs`/`timing.rs`.
2. Sửa lời chỉ lưu nháp. Nghe mới tạo lại WAV; đổi mốc giữ WAV, cập nhật lịch căn của các câu lân cận có liên quan.
3. Đổi kiểu chữ không gọi dịch/TTS; đổi giọng làm cũ WAV tương ứng, không dịch lại.
4. Cho mở trình sửa từ nguồn + nháp, không bắt buộc có MP4 kết quả. Nháp và bản xuất đang xem phải được phân biệt rõ.

Nghiệm thu: sửa một câu, nghe và đổi mốc không gọi hàm ghép toàn video; WAV của câu không đổi giữ nguyên; bản xuất trước vẫn mở được.

## Giai đoạn 3 — Preview theo yêu cầu

1. Đưa vị trí phát/vùng người dùng chọn xuống bộ điều phối. Phát vùng sẵn sàng, dịch/đọc trước khoảng 30–60 giây; con số chính thức chọn sau phép đo.
2. Tua sang vùng mới thì ưu tiên vùng đó, bỏ tác vụ chưa chạy không còn cần; phần đã xong vẫn giữ. Không sinh nhiều request/worker không giới hạn khi tua liên tục.
3. Chỉ bật TTS khi người dùng nghe giọng lồng; xem bản dịch chỉ cần chữ. Khi dừng phát, dừng nạp thêm vùng sau phần đệm; cho phép hoàn tất lô đang chạy.
4. Preview không cần `dub.wav` toàn video. Căn cục bộ phải dùng cùng quy tắc timeline với xuất, tính câu trước/sau có liên quan; khi phần sau chưa đủ thì đánh dấu nháp, không hứa khớp bản cuối tuyệt đối.
5. Tên/thuật ngữ: cho xem nhanh với trạng thái chưa duyệt. Trước bản dịch chính thức, xử lý tên và thuật ngữ nếu bật; thay đổi phải vô hiệu hóa phần phụ thuộc. Nếu chưa xác định được phạm vi ảnh hưởng thì dịch lại toàn phần phụ thuộc, không âm thầm giữ cache cũ.

Nghiệm thu: chỉ xem chữ không sinh WAV; xem đoạn đầu không tự dịch/đọc hết video dài; tua/dừng/đổi nguồn không phát sai câu; tên đã sửa xuất hiện thống nhất trong lời và tiếng đã chuẩn bị lại.

## Giai đoạn 4 — Xuất từ nháp ở bất kỳ thời điểm nào

1. Nút Xuất không phụ thuộc việc preview đã hoàn tất. Khi bấm, chuyển quyền điều phối từ preview sang xuất, không chạy hai pipeline tranh cùng cache.
2. Chốt revision/cấu hình cho lượt xuất; giai đoạn đầu khóa sửa nháp trong lúc xuất, vẫn giữ bản xuất cũ để mở lại khi an toàn.
3. Hoàn tất ASR, duyệt tên lần đầu, thuật ngữ nếu bật, dịch và WAV còn thiếu theo loại kết quả. Video phụ đề không chạy TTS/fit; audio không dựng hình.
4. Căn toàn bộ, trộn, xuất bằng đường hiện có; giữ NVENC/CPU dự phòng, file partial, kiểm tra file và lịch sử bản xuất.
5. Giữ thao tác Chạy toàn bộ cho hàng đợi và người muốn nhận file thẳng. Cập nhật UI/hướng dẫn cùng lúc; không thêm màn tùy chọn model dày đặc.

Nghiệm thu: xuất từ nháp mới có vài câu vẫn ra đủ video; cache hợp lệ không bị dịch/đọc lại; hủy/lỗi không mất bản nháp hay bản xuất trước; không coi nháp thiếu là kết quả hoàn chỉnh.

## Giai đoạn 5 — Kiểm chứng và đo tốc độ

- Test tích hợp trạng thái nháp, revision, hủy/phục hồi, phạm vi invalidation, preview theo vùng và xuất từ nháp thiếu.
- Kiểm tra nguồn local, YouTube/TikTok có/không có phụ đề và Douyin theo đúng đường tải hiện có; không thay cơ chế tải trong công việc này.
- Kiểm tra Không dịch, Video phụ đề, Audio, đổi ngôn ngữ/giọng, tên/thuật ngữ, tách/gộp và mốc sửa, việc cũ, hàng đợi, NVENC dự phòng CPU.
- Đo cùng nguồn và lời dịch cố định: tới hình đầu, chữ dịch đầu, giọng đầu, mở lại nháp, sửa/nghe một câu và xuất từ nháp. Báo riêng tải, ASR, dịch/TTS và dựng; tách cache lạnh/ấm và thời gian chờ người dùng.
- Đối chiếu preview/bản xuất về đủ chữ, WAV, mốc tiếng và câu bị cắt; FFprobe/giải mã kiểm tra kết quả. Chạy bộ kiểm tra local phù hợp sau khi tích hợp.

Thứ tự triển khai: 1 → 2 → 3 → 4 → 5, kiểm chứng từng giai đoạn trước khi chuyển hành vi mặc định. Tiêu chí chính là giảm công việc không được yêu cầu và thời gian tới lúc người dùng kiểm tra được nội dung; chưa cam kết số phút xử lý mới trước khi đo.
