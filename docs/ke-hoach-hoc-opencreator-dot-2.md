# Kế hoạch học OpenCreator — đợt 2

Ngày lập: 28/10/2026. Trạng thái: kế hoạch, chưa triển khai. Tham chiếu OpenCreator tại commit `d17bdc16d431db7a45b138c3180bdae533df003a`.

## Mục tiêu và ranh giới

Nâng độ tin cậy của bản dịch, giảm thao tác sửa lặp lại và giúp phần xem trước phản ánh đúng bản xuất. Đợt này nối tiếp [kế hoạch trước](ke-hoach-hoc-opencreator.md); không làm lại phụ đề an toàn, timeline, tách/gộp câu hoặc lịch sử bản xuất đã có.

Giữ Tauri 2, React, Rust, tên sản phẩm và giao diện tiếng Việt hiện tại. Audio/video xử lý trên máy; chỉ lời thoại đi qua dịch vụ dịch người dùng chọn. Không thêm dịch vụ, model, nhân bản giọng, tách người nói hoặc agent sáng tạo. Không tự tải công cụ, font hay model. Không mặc định bật nhiều vòng AI kiểm tra toàn video. Không commit trong phạm vi lập kế hoạch.

Phối hợp với [kế hoạch giao diện](ke-hoach-giao-dien-2026-10-28.md): chức năng mới đặt trong không gian chỉnh sửa và cấu hình hình/chữ đã dự kiến, không đồng thời thiết kế lại điều hướng. Mỗi mốc phải dùng được với giao diện hiện có nếu đợt giao diện chưa xong.

## Hiện trạng đã đối chiếu

- `ollama.rs` có `wrong_language` và `strip_stray_script`. Hai đường dịch trong `pipeline/translate.rs` có thể xóa chữ khác hệ rồi chấp nhận phần còn lại sau các lần thử thất bại. Đây là nguy cơ mất nội dung, chưa phải bằng chứng mọi bản dịch hiện có đều lỗi.
- Thuật ngữ trong `pipeline/translate.rs` lấy phần đầu lời thoại đến khoảng 6.000 ký tự; `terms_for_line` dùng phép tìm chuỗi con, chưa có phạm vi nghĩa theo câu.
- `work/script.tsx` có sửa từng câu và liên kết phát video. `editing.rs`, `timing.rs`, `pipeline/retake.rs` đã có định danh, phiên bản và phục hồi; cần tái sử dụng khi sửa hàng loạt.
- `pipeline/captions.rs` tạo ASS bằng ước lượng độ rộng; phần cấu hình kiểu chữ ở cửa sổ có thể khác cách FFmpeg xuống dòng. Đoạn thử 8 giây đã dùng FFmpeg và cache hiện có.
- `pipeline/media.rs` dùng lại `transcript.json` khi hoàn chỉnh. Chưa thấy checkpoint nhận dạng theo từng đoạn ở đường này; phải rà thêm cả `scripts/faster_whisper.py` trước triển khai.
- Đã có rút gọn câu sau khi đo WAV thật, tối đa hai vòng. Không thay thuật toán này chỉ để giống OpenCreator.

## Thứ tự và điều kiện chuyển mốc

| Mốc | Kết quả | Điều kiện hoàn tất |
| --- | --- | --- |
| A | Không mất chữ do bộ lọc ngôn ngữ | Không đường dịch nào nhận bản đã xóa chữ; câu lỗi chạy tiếp được |
| B | Tìm/thay lời dịch có xem trước và hoàn tác | Chỉ câu đã đổi cần đọc lại; không sửa nhầm phiên bản |
| C | Xem trước hình/chữ khớp cách xuất | Thử khả thi Windows đạt yêu cầu, có đối chiếu khung hình |
| D | Thuật ngữ phủ toàn video, đúng phạm vi | Đo được chất lượng và chi phí trên cùng bộ mẫu |
| E | Chép lời video dài tiếp tục theo đoạn | Dừng/mở lại chỉ chạy đoạn thiếu, không mất/lặp lời ở ranh giới |

Thực hiện A → B → C → D → E. A và B là đợt triển khai đầu. C có quyết định kỹ thuật riêng sau thử nghiệm; nếu chưa đạt thì giữ đoạn thử FFmpeg, ghi rõ giới hạn và có thể tiếp tục D. Không ấn định thời gian hoàn thành trước khi đo trên máy hiện tại.

## Mốc A — Bảo toàn chữ và thể hiện câu dịch cần kiểm tra

**Hành vi mong muốn:** tên riêng hoặc chữ khác hệ không bị âm thầm xóa. Người dùng biết câu nào chưa có bản dịch dùng được và có thể xử lý riêng câu đó.

1. Rà mọi đường nhận bản dịch: dịch trên máy, dịch API, DeepSeek local, Xem trước và Nghe trước; xác định phần dùng chung và nhánh xử lý riêng. Bỏ nhận kết quả bằng cách xóa ký tự khác hệ chữ ở cả hai nhánh hiện có.
2. Tách lỗi cấu trúc chắc chắn (JSON sai, ID thiếu/trùng, câu không có nội dung) khỏi nghi vấn ngôn ngữ/nội dung. Chữ khác hệ, tên riêng, URL hoặc nguyên văn giống nguồn không đủ để kết luận sai nghĩa. Bộ kiểm tra ngôn ngữ chỉ góp tín hiệu và hướng dẫn thử lại.
3. Giữ nguyên ứng viên để kiểm tra, với trạng thái và lý do riêng; ứng viên chưa được chấp nhận không được tính là câu dịch xong hoặc đưa vào TTS/bản xuất mới. Không ghi câu lỗi thành chuỗi rỗng rồi cho pipeline đi tiếp.
4. Giữ ngân sách thử lại hữu hạn theo luồng hiện có. Hết ngân sách mà vẫn không có bản dùng được thì dừng rõ tại câu lỗi, giữ câu đã xong và bản xuất cũ. Với xem/nghe trực tiếp, dừng ở phần chưa xử lý và báo lỗi; không đọc câu đã bị cắt chữ.
5. Cho xem lời gốc và ứng viên, sửa tay hoặc chủ động thử dịch lại riêng câu lỗi bằng dịch vụ/model của việc đó. Thao tác chấp nhận nguyên ứng viên phải rõ ràng. Không tự đổi dịch vụ hoặc bật vòng rà nghĩa toàn video.
6. Lưu trạng thái có phiên bản và dấu vân tay theo câu nguồn, ngôn ngữ, model/dịch vụ và phiên bản quy tắc liên quan. Không lưu key. Thay trạng thái chỉ làm mất cache tiếng nếu chữ được chấp nhận thực sự đổi.

**Tương thích:** không xóa hàng loạt bản dịch cũ. Việc cũ thiếu trạng thái vẫn đọc được và được ghi nhận là chưa có thông tin kiểm tra mới. Không thể tự khôi phục chữ từng bị bộ lọc cũ xóa; muốn kiểm tra lại phải làm rõ thao tác và giữ bản cũ. Các file ứng viên không được lẫn với `translation.json` đã chấp nhận.

**Nghiệm thu:** tên Trung/Nhật trong câu Việt, tên Latin/URL, câu Việt không có ký tự dấu đặc trưng, câu ngắn, câu trộn ngôn ngữ sai thật, JSON lỗi, 429, hủy và chạy tiếp. Fixture phải chứng minh chữ đầu vào ứng viên được giữ nguyên, câu đã xong không bị gọi lại, câu lỗi không vào TTS, Không dịch luôn dùng lời gốc. Kiểm tra thực tế một dịch vụ đang có; các dịch vụ chưa chạy thật phải ghi rõ.

**Vùng mã:** `ollama.rs`, `pipeline/translate.rs`, `job.rs`, `watch.rs`, `listen.rs`, `work/script.tsx`, `work/types.ts`, các IPC liên quan.

## Mốc B — Tìm và thay lời dịch hàng loạt

**Hành vi mong muốn:** sửa một cách dịch lặp lại trong nhiều câu, xem chính xác phần đổi trước khi tạo lại tiếng.

1. Có ô tìm, số lần khớp, câu trước/sau và phạm vi lời gốc/lời dịch/cả hai. Bấm kết quả phát đến câu; danh sách dài không tạo một lần toàn bộ thành phần nặng.
2. Đợt đầu chỉ thay lời dịch/lời đọc trong lớp biên tập. Lời nguồn chỉ dùng tìm và đối chiếu; sửa tên tiếp tục qua chức năng Tên, không ghi ngược lời Whisper.
3. Hỗ trợ thay lần hiện tại, các câu chọn hoặc toàn bộ kết quả tìm; mặc định tìm chuỗi thường, không regex. Có phân biệt hoa/thường. Hiển thị số câu ảnh hưởng và bản trước/sau trước khi áp dụng.
4. Áp thay đổi vào nháp, hỗ trợ hoàn tác cả lô và hủy trước khi xuất lại. Chốt quy tắc Unicode/khoảng trắng; offset tìm phải trỏ đúng bản chữ đang hiển thị. Không cắt cụm ký tự tổ hợp khi thay. Chuỗi thay chứa `$`, dấu gạch hoặc ký hiệu regex vẫn là chữ thường.
5. Gắn thao tác với ID ổn định và revision. Tách/gộp câu, đổi video hoặc phản hồi IPC cũ làm thay đổi revision thì từ chối nháp đã cũ; yêu cầu tìm lại, không áp theo vị trí mảng cũ.
6. Dùng giao dịch sửa câu hiện có để tạo lại tiếng. Giữ WAV của câu không đổi, tính lại căn tiếng/phụ đề/bản trộn cần thiết; lịch sử bản xuất ghi lý do sửa hàng loạt. Khi bận xử lý cùng việc thì khóa thao tác ghi.

**Tương thích:** không đổi tên WAV theo thứ tự sau khi thay chữ. Câu rỗng sau thay phải theo quy tắc sửa câu hiện có và báo rõ trước áp dụng; không coi xóa hết lời là bỏ câu hợp lệ ngầm định. Bản đang phát chỉ đổi sau xuất thành công.

**Nghiệm thu:** thay nhiều vị trí trong một câu, từ lặp, chữ tổ hợp tiếng Việt, CJK, dấu câu, chuỗi đặc biệt, thay bằng cùng nội dung, hoàn tác cả lô, tách/gộp sau tìm, chuyển video, lỗi/hủy lúc TTS. Đối chiếu hash WAV không liên quan và xác nhận không gọi Whisper/dịch khi chỉ sửa tay.

**Vùng mã:** `work/script.tsx`, phần tìm/thay tách riêng, `work/types.ts`, `editing.rs`, `pipeline/retake.rs`, `history.rs`.

## Mốc C — Dùng chung dữ liệu phụ đề cho xem trước và xuất

**C0 — thử khả thi trước:** tạo mẫu tối thiểu libass WASM trong Tauri/WebView2 trên Windows bằng ASS do Rust hiện tại tạo. Kiểm tra Worker/WASM/CSP, truy cập font local đúng quyền, bộ nhớ và đóng/mở cửa sổ chỉnh. Ghi nguồn, giấy phép, kích thước dependency và mức tăng gói. Không sao chép kiến trúc Electron của họ.

Chỉ tích hợp nếu chạy được offline, ổn định và cải thiện rõ độ khớp. Không tự tải font hoặc đưa dependency chưa đánh giá vào đường mặc định; nếu không đạt thì giữ đoạn thử FFmpeg và ghi kết luận thử nghiệm.

**C1 — tích hợp khi C0 đạt:**

1. Rust tạo một gói xem trước gồm ASS, kích thước đầu ra, danh sách font và hash từ cùng đường dựng phụ đề dùng cho bản xuất/đoạn thử. Chỉ có một quy tắc chia đoạn, thời gian và escape ASS.
2. Cửa sổ dựng lớp chữ ở kích thước đầu ra rồi co cả lớp theo khung xem. Đồng bộ khi phát, dừng, tua, đổi tốc độ và toàn màn hình; không khởi động FFmpeg hay gọi IPC ở mỗi khung hình.
3. Dùng cùng file font giữa hai bên; thiếu glyph phải báo hoặc có fallback xác định từ font đã có. Không tự tìm font online. Font người dùng không có quyền đóng gói phải được xử lý theo nguồn hợp lệ khi phát hành.
4. Cache theo chữ/mốc, cấu hình, kích thước, phiên bản bộ dựng và hash font. Phản hồi chuẩn bị cũ không ghi đè mẫu mới; hủy/đóng phải dọn Worker và tài nguyên.
5. Giữ tương thích `render.json` và tọa độ kiểu chữ cũ; đổi riêng kiểu chữ không làm lại dịch/TTS. Không áp bố cục bản xuất lên Xem trước dịch trực tiếp nếu chưa hỗ trợ đủ; vẫn ghi rõ giới hạn đường này.

**Nghiệm thu:** so ASS tạo cho mẫu và xuất; so khung hình ở cùng mốc với font thực tế, câu dài, dấu tổ hợp, 9:16/16:9/1:1, xoay và SAR khác 1, chồng câu, viền/nền/vị trí chữ. Đánh giá riêng chữ/xuống dòng/vị trí và sai số raster/codec; không hứa RGB giống hoàn toàn. Thử cả bản dev và EXE đã đóng gói trên Windows, kiểm tra đổi nhanh kiểu chữ và mở/đóng nhiều lần.

**Vùng mã:** `pipeline/captions.rs`, `pipeline/sample.rs`, `pipeline/assemble.rs`, `work/StyleEditor.tsx`, Tauri asset/CSP và module Worker mới nếu cần.

## Mốc D — Thuật ngữ theo ngữ cảnh toàn video

**Hành vi mong muốn:** video dài không chỉ hiểu phần mở đầu; một từ nhiều nghĩa không bị ép cùng một cách dịch ở mọi câu.

1. Chỉ chạy khi người dùng đã bật thuật ngữ. Chia toàn bộ lời thành các phần liên tiếp theo ngân sách request, không cắt bỏ phần cuối để vừa prompt. Các phần là đơn vị xử lý, không tự gọi là cảnh hoặc chương thật.
2. Mỗi phần có tóm tắt và thuật ngữ kèm câu nguồn áp dụng. Kiểm tra ID, sự xuất hiện của từ và xung đột nghĩa. Với ngôn ngữ có khoảng trắng, không nhận một từ chỉ vì nó là chuỗi con của từ khác; giữ hỗ trợ ngôn ngữ không cách từ.
3. Mỗi lô dịch chỉ nhận thuật ngữ liên quan và ngữ cảnh đủ dùng. Chỉ thêm bước gộp tóm tắt nếu cần và đã đo ngân sách. Không mặc định thêm vòng AI sửa/nghiệm chứng mọi câu như repo tham khảo.
4. Cache theo nội dung, ngôn ngữ, model/dịch vụ, endpoint và prompt. Checkpoint từng phần đã xong; chỉ gọi phần còn thiếu khi chạy tiếp. Không ghi key. Đổi kiểu chữ không làm mất cache này.
5. Lỗi phân tích tùy chọn giữ phần hợp lệ và báo thiếu ngữ cảnh; hủy phải dừng. Lỗi xác thực/hạn mức cần phân loại rõ để tránh lặp request vô ích. Giữ DeepSeek local một request mỗi lần theo giới hạn hiện tại.
6. Với việc cũ, đọc `terms.json` cũ bình thường. Làm rõ lựa chọn nâng cấp phân tích; không vì mở việc cũ mà tự bỏ bản dịch/WAV đã có. Phân tích mới chỉ ảnh hưởng bản dịch khi người dùng chủ động yêu cầu dịch lại.

**Nghiệm thu:** bộ mẫu có thuật ngữ chỉ xuất hiện cuối video, một từ nhiều nghĩa, số/đơn vị, tên riêng và chữ không cách từ. So cùng nguồn/model với cách hiện tại: tính nhất quán qua người đọc, lỗi còn lại, thời gian và số lượt gọi. Không dùng chính model dịch làm bằng chứng duy nhất rằng chất lượng tăng. Nếu không cải thiện đủ so chi phí thì chưa đổi đường mặc định.

**Vùng mã:** `pipeline/translate.rs`, `ollama.rs`, `job.rs`, cấu trúc `terms.json` có phiên bản và cache phân tích riêng.

## Mốc E — Checkpoint Whisper cho video dài

**Hành vi mong muốn:** đóng/dừng giữa video dài rồi chạy tiếp không phải chép lại những đoạn đã hoàn tất.

1. Rà riêng whisper-cli và faster-whisper CUDA để chọn cách cắt đoạn phù hợp. Đo độ dài đoạn và chi phí khởi động model; tránh khiến faster-whisper nạp model lại cho mỗi đoạn nếu lợi ích không bù được.
2. Lưu manifest có phiên bản: định danh nguồn, cấu hình ASR, ngôn ngữ, model thực tế, mốc đoạn và kết quả đã xác minh. Fingerprint phải phân biệt thay nội dung nguồn/model, không chỉ tên file hoặc kích thước. Không dùng kết quả nửa file.
3. Chọn điểm cắt gần khoảng lặng và giữ tọa độ tuyệt đối. Nếu cần overlap thì quy định rõ cách ghép/khử trùng; không xóa chữ chỉ vì văn bản lặp giống nhau. Giữ câu và timestamp ở ranh giới theo bằng chứng nhận dạng.
4. Ghi từng checkpoint nguyên tử. Chỉ chốt `transcript.json` khi đủ đoạn, đúng thứ tự và mốc hợp lệ. Sau đó mới dọn audio trung gian theo quy tắc hiện tại; giữ dữ liệu cần cho chạy tiếp khi chưa xong.
5. Đợt đầu áp cho xuất qua `start_dub`; không âm thầm thay cơ chế Whisper chạy một lần của Xem trước/Nghe trước. Việc đang dở mở lại phải chọn đúng cơ chế, không trộn hai bộ timestamp.
6. Việc cũ có transcript hoàn chỉnh vẫn bỏ qua nhận dạng. Việc cũ chưa có checkpoint được chép lại theo đường đã chọn và báo rõ giới hạn; không suy đoán dữ liệu từ file tạm chưa kiểm chứng.

**Nghiệm thu:** video dài đủ qua nhiều đoạn, cắt giữa từ/câu, khoảng lặng dài, câu lặp thật, đoạn cuối ngắn, hủy giữa đoạn, tắt app/mở lại, lỗi ghi/thiếu dung lượng, đổi model và nguồn khác cùng tên. So bản chạy một lượt và bản chạy tiếp về đủ lời, lặp lời, mốc và thời gian; ghi riêng kết quả whisper-cli/faster-whisper. Không bắt tải model để hoàn tất thử nghiệm.

**Vùng mã:** `pipeline/media.rs`, `job.rs`, `scripts/faster_whisper.py`, module checkpoint mới nếu cần. `watch.rs`/`listen.rs` cần kiểm tra hồi quy dù chưa chuyển sang nhận dạng theo đoạn.

## Kiểm chứng và bàn giao

- Mỗi mốc có fixture cho rủi ro dữ liệu cụ thể, kiểm tra frontend/Rust đúng phạm vi và một lượt dùng thật khi thay hành vi video/giọng. Dùng `npm run verify:local` ở mốc tích hợp; không chạy build/test cho thay đổi chỉ có tài liệu.
- So hash lời gốc, bản dịch và WAV không liên quan trước/sau; mô phỏng lỗi, hủy, chạy tiếp và phản hồi IPC sai thứ tự ở nơi cần thiết.
- Hồi quy các chế độ video/audio/chỉ chữ/Không dịch, chỉnh mốc/tách gộp, hàng đợi, đoạn thử, lịch sử và mở việc cũ. Đường trực tiếp giữ giới hạn đã công bố.
- Bàn giao từng mốc gồm thay đổi, ảnh/video trước-sau khi có ý nghĩa, chi phí đo được, cache còn dùng được và nhánh chưa thử. Không coi test cấu trúc là chứng minh chất lượng dịch hoặc giọng.
- Nghiệm thu installer trên Windows sạch và các dịch vụ chưa thử vẫn là phần việc riêng; kế hoạch này không thay thế chúng.

## Nguồn tham khảo cố định

Học thiết kế, không mặc định sao chép code/dependency. Kiểm tra giấy phép và giữ thông báo cần thiết nếu triển khai có tái sử dụng mã.

- [Kiểm tra bản dịch](https://github.com/krillinai/OpenCreator/blob/d17bdc16d431db7a45b138c3180bdae533df003a/runtime/krillinai/internal/service/translation_validation.go)
- [Tìm/thay phụ đề](https://github.com/krillinai/OpenCreator/blob/d17bdc16d431db7a45b138c3180bdae533df003a/apps/web/src/features/dashboard/subtitle-search.ts)
- [Xem trước và bản xuất: triển khai, đo đạc và giới hạn](https://github.com/krillinai/OpenCreator/blob/d17bdc16d431db7a45b138c3180bdae533df003a/docs/development/subtitle-preview-burnin-validation.md)
- [Ngữ cảnh và thuật ngữ](https://github.com/krillinai/OpenCreator/blob/d17bdc16d431db7a45b138c3180bdae533df003a/runtime/krillinai/docs/subtitle-semantic-context.md)
- [Mã thuật ngữ theo phạm vi câu](https://github.com/krillinai/OpenCreator/blob/d17bdc16d431db7a45b138c3180bdae533df003a/runtime/krillinai/internal/service/translation_semantic_context.go)
- [Checkpoint chép lời](https://github.com/krillinai/OpenCreator/blob/d17bdc16d431db7a45b138c3180bdae533df003a/runtime/krillinai/internal/service/transcription_checkpoint.go)
