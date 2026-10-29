# Xử lý khi cần — đợt 3, ngày 29/10/2026

Đợt này nối xác minh cache chữ/giọng và quyền sở hữu tác vụ vào pipeline hiện tại. Đã có phần điều phối độc quyền của giai đoạn 1; chưa có điều phối yêu cầu theo vùng hoặc luồng mặc định mở nguồn rồi chỉ xử lý khi xem/nghe/xuất. Chưa công bố hoàn tất nghiệm thu giai đoạn 1 với model/app thật.

## Cache chữ và thuật ngữ

`text_cache.rs` ghi `translation-cache.json` và `terms-cache.json` phiên bản 1 cạnh các file cũ. Dấu xác nhận gồm hash dữ liệu phụ thuộc, cách dịch và hash đầu ra từng câu (hoặc cả bảng thuật ngữ); không lưu API key. Tên model trống được quy về model mặc định thực tế trước khi xác lập cấu hình nháp.

- Đầu vào gồm nguồn, lời chép/cách chia câu, ngôn ngữ, Không dịch, thuật ngữ, danh sách sửa tên thực tế, dịch vụ/model và ngân sách theo mốc khi dịch video. Đổi xác nhận tên từ chưa duyệt sang đã duyệt nhưng danh sách rỗng không tự vô hiệu hóa bản dịch. Đổi riêng giọng, hình/chữ hay mốc sửa không buộc dịch lại.
- Trước khi dịch, xác minh từng câu đã lưu. Chỉ câu có hash đúng và còn nội dung mới được dùng lại. Câu chưa dịch không nhận dấu xác nhận thành công. Thiếu/xóa file bản dịch hoặc xóa nội dung câu không được giữ WAV cũ để xuất nhầm tiếng.
- Không đủ căn cứ dùng lại thì cất chữ/dấu xác nhận/WAV bị ảnh hưởng trong `cache-stale/<mã>/`, cất bản xuất cũ qua đường `cu/` hiện có, vô hiệu hóa dữ liệu ghép phụ thuộc rồi mới chốt cache đã làm sạch. Câu khác còn hợp lệ giữ nguyên. Cache cũ chưa có dấu xác nhận được xử lý thận trọng theo đường này; có thể phải dịch/đọc lại một lần.
- Giữ dấu xác nhận cũ cho tới khi vô hiệu hóa xong. Nếu dừng giữa các lần ghi, đầu vào/đầu ra không khớp sẽ bị kiểm tra lại. File nền và dấu xác nhận vẫn là hai commit riêng, chưa phải giao dịch nhiều file. Bản ghi hỏng hoặc khác phiên bản báo lỗi và được giữ nguyên.
- Chốt từng cụm dịch phải khớp lượt chạy và dữ liệu phụ thuộc đã giữ lúc bắt đầu. Không ghi phản hồi của lượt đã bỏ qua. Giữ thứ tự lưu cụm xong rồi mới đưa sang hàng chờ giọng có giới hạn hai lô.
- Lời sửa qua app và lời rút gọn cho vừa thời lượng được ký lại như nội dung đã biên tập dưới cùng đầu vào dịch, tránh coi chúng là cache bị sửa ngoài app. Giao dịch sửa bản xuất sao lưu/phục hồi cả dấu xác nhận chữ khi lỗi/hủy; schema phục hồi cũ vẫn đọc được. Cấu trúc `editing.json` giữ nguyên, không bị tự chuyển về câu gốc để dịch lại.
- Nút kiểm tra kết quả có sẵn nhận cả dịch vụ/model của việc đang chọn (kể cả snapshot hàng đợi), đối chiếu cấu hình và dấu xác nhận chữ. Không mở nhầm kết quả của dịch vụ/model cũ chỉ vì ngôn ngữ/giọng còn giống nhau. Bản xuất cũ vẫn mở từ thư viện.

## Cache giọng

Trước khi cần dùng lại WAV trong pipeline, `prepare_speech_cache` kiểm tra khóa lời đọc và SHA-256 WAV, cất file không khớp rồi tạo lại. Sửa lời chỉ làm cũ phần phụ thuộc; đổi công thức giọng có thể làm cũ toàn phần liên quan. Các file không bị ảnh hưởng giữ nguyên.

Khi đã có bản xuất và công thức giọng vẫn khớp, thiếu file WAV từng câu không tự phủ nhận bản xuất đã hoàn tất. Khi cần dựng lại từ WAV thì file thiếu/sai phải được tạo lại. Đường mở kết quả có sẵn cũng đối chiếu hash nội dung model/config giọng. Thay cặp giọng tại cùng tên file không được âm thầm coi là giọng cũ.

Giọng lấy từ cấu hình nháp đã giữ cho lượt chạy, thay vì đọc lựa chọn toàn app lại ở mỗi lô. Các câu im lặng có lời hợp lệ cũng có thể được chốt dấu xác nhận, tránh phải làm lại chỉ vì không có âm có thể đọc.

Preview vẫn chỉ công bố `lines.json`, `translation.json` và WAV chuẩn khi hoàn tất; hủy giữa chừng vẫn theo hợp đồng dọn `watch/` cũ. Preview mới không dùng chữ cache cũ được ký sau khi hoàn tất: xuất có thể dùng lại lời đó, giữ nguyên quy tắc rút gọn/căn khi xuất. WAV preview được xác minh khi công thức đọc tương ứng. Bản preview cũ/nhánh mở lại đã có file không tự được nâng thành cache đã xác minh chỉ nhờ `done.json`; xuất có thể cất và làm lại phần chưa có căn cứ. Đổi tên/thuật ngữ thực sự vẫn làm cũ phần dịch phụ thuộc.

## Quyền sở hữu tác vụ và hủy

`coordinator.rs` sở hữu một tác vụ tại một thời điểm, có mã lượt, loại tác vụ và trạng thái đang dừng (`task_state`). Lồng toàn bộ, chạy lại, preview, tải/chuẩn bị nguồn, tải hình bổ sung, nối audio, sửa bản xuất/tên, đốt chữ, dựng đoạn thử, thử giọng và dịch phần nghe đều lấy lease từ cùng bộ điều phối. Các thao tác chỉ đọc và trình tải Whisper độc lập giữ đường cũ.

- Lease chỉ trả khi worker kết thúc hoặc tháo stack; lỗi/panic cũng không để kẹt quyền. Hủy đặt cờ, thu hồi TTS và gửi sự kiện trong cùng vùng sở hữu, không trả quyền cho việc mới khi luồng cũ còn chạy.
- Chốt snapshot kết thúc trước khi trả quyền và gửi sự kiện bằng snapshot đã chốt. Cleanup cũ không mở khóa/ghi đè snapshot của tác vụ mới. Các lệnh không còn tự đặt lại cờ hủy sau lúc đã nhận quyền, tránh nuốt yêu cầu Dừng đến sớm.
- Bỏ luồng HTTP chạy ngầm sau khi hàm dịch đã trả về. Request đang gửi vẫn thuộc worker hiện tại; hủy bỏ phản hồi, ngừng retry/backoff và giữ quyền tới khi kết nối trả về hoặc timeout. DeepSeek local có thêm khóa một request tại một thời điểm.
- Việc này có thay đổi quan sát được: **Dừng có thể chờ HTTP hiện tại kết thúc**, theo timeout sẵn có (thường tối đa 180 giây; DeepSeek local 300 giây), thay vì báo xong trong khi request vẫn chạy ngầm. Đây không phải cơ chế ngắt socket HTTP tức thì. UI báo đang dừng; Xem trước/Nghe trước tạm dừng phát ngay khi bấm Dừng.

## Kiểm chứng và phần tiếp theo

Test bổ sung bao phủ mở lại bản dịch dở, hash câu bị đổi/xóa/file bị mất, đổi model/thuật ngữ/ngân sách, giữ lời sửa tay, phục hồi dấu xác nhận khi retake lỗi, preview mới sang xuất, response cũ, cache khác phiên bản, WAV hỏng, giữ bản xuất khi chỉ thiếu WAV gốc và hash model/config giọng. Test điều phối kiểm tra hủy vẫn giữ quyền, cleanup cũ, panic và thứ tự thu hồi trước lượt sau. Test HTTP dùng fake worker và server loopback trả 429, không gọi dịch vụ thật.

`npm run verify:local` qua cả sáu nhóm: frontend (95 test), hàng đợi, playback, kiểm tra gói Windows, build frontend và Rust offline (325 qua, 0 lỗi, 4 bỏ qua). Báo cáo: [report.json](../.bench-asr/verify-1793254852198/report.json). Sau lượt đó, chỉnh khóa tên để xác nhận danh sách rỗng không làm cũ WAV rồi chạy lại toàn bộ Rust: vẫn 325 qua, 0 lỗi, 4 bỏ qua. `git diff --check` không có lỗi khoảng trắng.

Chưa nghiệm thu qua cửa sổ app với model/dịch vụ thật, đóng cưỡng bức giữa các commit hoặc mất điện. Hash/cache không thay validation ngôn ngữ và mốc hiện có. Không đo hoặc hứa tốc độ mới; việc quét hash/giữ HTTP đang dừng có thể tăng thời gian chờ trong một số tình huống.

Các bước tiếp theo vẫn là tách lưu sửa/nghe câu/xuất (giai đoạn 2), điều phối vùng xem/nghe và ưu tiên tua (giai đoạn 3), xuất từ nháp thiếu (giai đoạn 4), rồi nghiệm thu/đo đầy đủ (giai đoạn 5). Chưa đổi mặc định thành xử lý theo yêu cầu.
