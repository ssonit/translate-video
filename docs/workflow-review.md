# Kiểm tra quy trình ngày 26/10/2026

## Phạm vi thay đổi

- Hàng đợi giữ dịch vụ, tên model cụ thể (kể cả khi đang chọn mặc định), thuật ngữ, ngôn ngữ, giọng và loại kết quả theo từng video. Không ghi API key vào hàng đợi. Rust lấy key đã lưu đúng dịch vụ, không đổi cấu hình dịch chung khi chạy một video trong hàng.
- Hàng đợi cũ vẫn đọc được. Việc chưa có cấu hình dịch lấy lựa chọn hiện tại khi bắt đầu; cấu hình đó được giữ lại cho lần thử lại. Có thể cập nhật một video đang chờ theo lựa chọn hiện tại.
- Các câu đã dịch vẫn dùng cache như trước; đổi dịch vụ/model trong hàng đợi áp dụng cho phần còn cần dịch, không tự xóa bản dịch đã có.
- Chọn đầu ra bằng ba lựa chọn loại trừ nhau. Chế độ phụ đề ẩn giọng; chỉ video lồng tiếng mới hiện tùy chọn kèm chữ. Có tóm tắt lựa chọn và nơi lưu.
- Tiến độ gồm bốn giai đoạn và phần chi tiết chín bước. Trạng thái chờ kiểm tra tên hiện ở cả màn tiến độ và danh sách video. Đồng hồ, ETA và log thời gian loại trừ thời gian chờ; trạng thái chờ được khôi phục khi tải lại cửa sổ.
- Xem trước và Nghe trước có mô tả những bước chưa áp dụng so với bản xuất.
- Căn tiếng dùng tối đa tám câu trong một lần mở FFmpeg, một luồng xử lý bộ lọc. Giữ nguyên bộ lọc, thứ tự trộn, giới hạn tăng tốc, cách cắt và định dạng cache. File tạm riêng được dọn khi thành công, lỗi hoặc hủy; WAV nguồn được giữ.

## Đo bằng dữ liệu thật

Chạy test `benchmark_real_job_fit_and_merge` trên 32 WAV đầu của một việc sẵn có trong `output`. Chỉ đọc dữ liệu nguồn; đầu ra thử nằm trong folder tạm riêng. Benchmark không gọi AI, không tải thêm dữ liệu và không thay video kết quả cũ.

| Phép đo | Kết quả |
| --- | --- |
| Căn từng câu bằng tiến trình riêng | 6,056 giây |
| Căn theo nhóm tám câu | 1,178 giây |
| Tỉ lệ nhanh hơn ở riêng bước căn tiếng | 5,14 lần |
| So sánh PCM của cả 32 câu | Giống hệt từng mẫu |
| Video thử ghép từ tiếng mới | 43,320 giây; ffprobe đọc được độ dài, FFmpeg giải mã thành công |

Số đo một lần trên máy hiện tại, chưa phải cam kết tốc độ của toàn pipeline. Các log việc thật trước đây có trường hợp căn tiếng chiếm 23–49% thời gian; đọc giọng vẫn thường chiếm phần lớn.

Chạy lại benchmark (PowerShell):

```powershell
$env:DUB_BENCH_JOB = 'D:\duong-dan\folder-viec-da-co-wav'
cargo test --manifest-path src-tauri/Cargo.toml benchmark_real_job_fit_and_merge --lib -- --ignored --nocapture
```

## Kiểm chứng và giới hạn

- `npm run build` thành công; bộ test frontend và Rust đều qua (Rust: 229 test qua, ba test bỏ qua ở lượt mặc định; benchmark đã chạy riêng thành công).
- Test PCM so sánh đường cũ/mới cho tốc độ thường, tăng tốc và cắt; kiểm tra dọn file tạm khi lỗi hoặc đã hủy.
- Test hàng đợi kiểm tra giữ cấu hình sau khôi phục, tương thích hàng cũ và loại bỏ trường key lạc vào dữ liệu.
- Test Rust kiểm tra lấy đúng key/model của dịch vụ trong hàng, không thay dịch vụ hiện tại và không cộng thời gian chờ hai lần.
- Test thời gian kiểm tra đồng hồ đứng yên trong khi chờ, bỏ ETA và trừ tổng thời gian chờ khi tiếp tục.
- Đã mở cửa sổ Tauri để kiểm tra bố cục ba loại đầu ra và việc ẩn giọng khi chọn video phụ đề.
- Chưa xác nhận toàn bộ đường Whisper → dịch → Piper → xuất trên một video hoàn toàn mới với mọi dịch vụ. Benchmark thực tế dùng WAV đã có và tập trung vào căn tiếng, ghép và giải mã video.
