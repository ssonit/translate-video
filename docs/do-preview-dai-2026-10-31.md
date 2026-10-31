# Đo cập nhật preview dài — 31/10/2026

## Phạm vi và tái lập

Chạy `cargo test --offline benchmark_verified_preview_snapshot -- --ignored --nocapture --test-threads=1` trong `src-tauri`. Benchmark được đánh dấu ignored để không tạo 2.185 WAV mỗi lần chạy hồi quy. Fixture nằm riêng trong thư mục tạm và được dọn khi test kết thúc. Không dùng hoặc thay đổi folder việc thật.

Máy Windows, AMD Ryzen 7 8845H, RAM khả dụng phần cứng 16.435.761.152 byte. Cargo test dùng profile debug, dependency offline hiện có. Không gọi mạng, dịch, TTS, FFmpeg hoặc GPU trong benchmark.

Phiên bản: Windows NT 10.0.26300.0; rustc 1.94.1 (e408947bf 2026-03-25), cargo 1.94.1 (29ea6fb6a 2026-03-24).

Fixture gồm 2.185 câu, cách nhau 3 giây, mỗi WAV PCM mono 16-bit 22.050 Hz dài 2 giây (88.244 byte); tổng WAV 192.813.140 byte. Nguồn chỉ là file định danh thử, không phải video phát được. Lời cố định; manifest chứa receipt khớp SHA-256 và speech key. Helper dựng fixture theo lô chỉ được biên dịch trong test; đường đo dùng nguyên `status`, kiểm tra hash và PCM của sản phẩm.

Harness so đường cũ ba lần status (view → clips → speech_slots) với đường dùng một snapshot. Đo phần lấy trạng thái và dựng clip, bỏ phần chung đọc metadata/ffprobe, emit IPC và cấp quyền asset. Đường cũ được mô phỏng riêng trong test, không checkout/revert mã người dùng. Chín cặp chạy luân phiên thứ tự; ngoài đồng hồ đo, đối chiếu toàn bộ JSON clip (ID, path, version, mốc, tốc độ, thời lượng phát và tentative).

File vừa được tạo nên số đo là cache hệ điều hành ấm. Lượt đầu cũng không được gọi là cache đĩa lạnh. Không xóa standby cache toàn máy; chưa có số đo cold disk hay UI end-to-end. Biến thiên bao gồm lịch CPU và I/O hệ điều hành.

## Giảm công việc

Chín cặp đo, đơn vị giây:

| Đường backend | Trung vị | Nhỏ nhất–lớn nhất | Lượt đầu sau dựng fixture |
| --- | ---: | ---: | ---: |
| Ba lần status | 52,433 | 50,302–52,986 | 50,302 |
| Snapshot chung | 21,962 | 21,895–22,857 | 22,857 |

Trung vị giảm 58,1% (tỉ số 2,39 lần). Tổng test gồm tạo fixture, đối chiếu ngoài đồng hồ và kiểm tra file hỏng là 836,58 giây. Dữ liệu từng lượt nằm trong `preview-benchmark-2026-10-31.json`. Cả chín cặp cho JSON clip tương đương; sau khi làm hỏng WAV đầu, trạng thái và clip đều còn đúng 2.184 câu.

Trong một publish, số lần status giảm 3 → 1; số WAV được băm/kiểm tra PCM giảm 6.555 → 2.185. Speech slots vẫn đọc thêm 2.185 WAV lấy độ dài. Theo kích thước fixture, lượng WAV đọc theo luồng mã giảm 771.252.560 → 385.626.280 byte. Đây là lượng đọc logic suy từ mã và kích thước file, không phải bộ đếm đọc đĩa vật lý hay ETW.

Snapshot chỉ sống trong lời gọi đồng bộ; cập nhật sau lấy status mới. Không lưu cache xác minh xuyên thao tác và không đổi receipt, source/revision hoặc phép kiểm SHA-256/PCM.

## Giới hạn còn lại

Chạy thêm `cargo test --offline benchmark_preview_components -- --ignored --nocapture --test-threads=1`, cùng loại fixture, ba lượt trong một tiến trình mới (cache file ấm):

| Thành phần | Trung vị ms | Nhỏ nhất–lớn nhất ms |
| --- | ---: | ---: |
| status, đủ hash/PCM | 14.639,7065 | 14.616,6038–14.691,6814 |
| clips dùng snapshot, gồm đọc lại WAV lấy độ dài | 6.778,4839 | 6.764,5476–6.825,6704 |
| Một vòng contains + find cho mỗi cue | 66,0327 | 64,8808–72,0282 |
| HashSet/HashMap, gồm tạo chỉ mục | 3,9455 | 3,8859–3,9643 |

Microbenchmark tra cứu đối chiếu cùng số kết quả, chỉ đo ID; không đại diện toàn bộ observe hoặc toàn bộ contains của clips. Phần tra cứu giảm khoảng 62 ms trên một vòng, nhỏ so với 21–22 giây cập nhật trong fixture này. Giữ thay đổi sản phẩm tập trung ở tái sử dụng snapshot; chưa đổi cấu trúc dữ liệu. Hai microbenchmark vẫn được giữ để chạy lại khi cần.

`observe` vẫn tìm cue cũ bằng find cho từng cue; clips/speech_slots vẫn tra Vec bằng contains, và speech_slots còn đọc lại WAV. Phần clips 6,78 giây chưa được tách thành riêng đọc PCM và đọc/tạo document, nên không quy toàn bộ thời gian đó cho WAV. `open` vẫn băm toàn nguồn qua inspect_source; benchmark này không đo mở video dài và không thay ranh giới xác minh nguồn.

Không suy ra tốc độ lồng toàn video, RAM/VRAM tối đa hoặc thời gian xuất từ số đo cập nhật backend. Chưa chạy mới UI, dịch hoặc TTS thật; bằng chứng nghiệm thu ngày 29/10 vẫn là bằng chứng lịch sử.

## Hồi quy mới chạy

`cargo test --offline -- --test-threads=1`: **340 đạt, 0 lỗi, 6 ignored**, 36,95 giây sau compile. Hai benchmark bổ sung nằm trong sáu ignored và đã được gọi riêng, đều đạt; bốn bài ignored cũ không được chạy trong đợt này. `git diff --check` đạt (Git chỉ cảnh báo chuẩn hóa LF/CRLF).

Test mới đối chiếu clip từ snapshot với đường clips công khai; thay một byte PCM vẫn giữ định dạng hoặc làm cụt WAV đều bị lần gọi tiếp theo loại khỏi clip. Hồi quy hiện có đạt các nhóm: sửa một câu chỉ đổi phiên bản media câu đó; đổi mốc giữ WAV; lưu/undo qua mở lại; nhận diện source/revision/receipt cũ; nguồn đã đổi không dùng cache xuất; WAV hỏng chỉ dựng lại đúng câu; chọn vùng có ngữ cảnh giới hạn; phiên đóng không xóa intent phiên mới; hủy giữ quyền tới khi worker trả về; kết quả TTS đến muộn không ghi sang việc khác. Đây là kiểm chứng backend tự động, chưa phải lượt tua/dừng/đóng cửa sổ bằng UI thật mới.

Không sửa frontend hoặc README trong đợt tối ưu này; giữ các thay đổi đã có trước ở README, DraftStudio, Studio.css, composer và báo cáo rà soát. Không commit.
