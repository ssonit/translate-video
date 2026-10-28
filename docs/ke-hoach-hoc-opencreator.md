# Kế hoạch hoàn thiện phụ đề và sửa bản xuất

Ngày lập: 28/10/2026. Mốc A, B, C và D đã triển khai và kiểm tra local; kiểm tra dịch tùy chọn chưa triển khai. Xem [báo cáo mốc A](phu-de-an-toan-2026-10-28.md), [mốc B](chinh-moc-cau-2026-10-28.md), [mốc C](tach-gop-cau-2026-10-28.md) và [mốc D](lich-su-ban-xuat-2026-10-28.md) để biết phạm vi và giới hạn.

## Mục tiêu và phạm vi

Học cách OpenCreator bảo toàn nội dung khi chia phụ đề, chỉnh thời gian và theo dõi bản sửa. Ưu tiên giúp người dùng sửa một video chưa đạt mà không chép lời, dịch hoặc đọc lại toàn bộ. App hiện đã đủ luồng chính; đây là đợt hoàn thiện chất lượng, không mở rộng thành bộ công cụ sáng tạo có agent.

Giữ Tauri, giao diện tiếng Việt hiện tại và xử lý hình/tiếng trên máy. Không đổi tên app, thêm dịch vụ, tự tải công cụ/model, nhân bản giọng hoặc chọn cách tách nhiều người nói. Không commit trong phạm vi lập kế hoạch. Các mốc dưới đây được triển khai riêng, nghiệm thu xong mới sang mốc kế tiếp.

## Điểm xuất phát trong project

- `pipeline/captions.rs` đã chia lời thành các đoạn khi ghi chữ, giữ SRT, bản dịch và WAV gốc. Cách tính độ rộng hiện là ước lượng ký tự, chưa phải đo chính xác bằng font.
- `work/script.tsx` đã có sửa lời, phát đến câu và tạo lại các câu chọn; `pipeline/retake.rs` có cơ chế giữ dữ liệu để phục hồi khi sửa lỗi/hủy. `LinePatch` hiện chỉ gồm chỉ số câu và chữ mới.
- `job.rs` lưu câu bằng thứ tự trong mảng; WAV dùng tên `lines/NNN.wav`. Tách/gộp câu cần giải quyết định danh trước, không chỉ đổi mảng trên giao diện.
- Đã có tên, thuật ngữ, ngữ cảnh dịch, cảnh báo câu và bản xuất cũ. Không làm lại các phần này từ đầu.
- Các báo cáo nghiệm thu hiện tại còn giới hạn về nguồn, dịch vụ và máy Windows sạch. Kế hoạch này không thay thế phần nghiệm thu phát hành còn thiếu.

## Mốc A — Chia phụ đề an toàn và chỉ ra đoạn cần xem lại

**Kết quả người dùng nhận được:** phụ đề dễ đọc hơn, không âm thầm mất hoặc lặp chữ; bấm cảnh báo để xem đúng đoạn.

1. Tách logic bố cục phụ đề thành dữ liệu có thể kiểm tra: câu nguồn, các đoạn hiển thị, mốc thời gian và cảnh báo. Dùng cùng đường dựng cho đoạn thử và bản xuất.
2. Chia theo ranh giới từ/cụm và dấu câu; không cắt giữa cụm ký tự Unicode. Giữ dấu tiếng Việt, chữ tổ hợp, tên, số, đơn vị và URL khi có thể. Trường hợp không đủ chỗ phải có cách xuống dòng/chia đoạn xác định và cảnh báo phù hợp.
3. Kiểm tra các đoạn ghép lại giữ nguyên nội dung đã chốt, chỉ cho phép chuẩn hóa khoảng trắng đã định nghĩa. Không gọi AI và không rút gọn lời để vừa khung.
4. Giữ các đoạn trong khoảng câu cha, thời gian hữu hạn, không âm và độ dài dương. Câu cha vốn chồng nhau phải được nhận diện riêng; không tự dời hoặc xóa lời để che lỗi.
5. Mở rộng cảnh báo hiện có với thời lượng đọc quá ngắn/chữ quá dày và lỗi mốc thời gian. Ngưỡng là gợi ý, cần điều chỉnh bằng mẫu thật; không dùng giới hạn của tiếng Trung cho tiếng Việt.
6. Lỗi không thể bảo toàn chữ hoặc thời gian thì báo rõ và giữ bản xuất cũ. Cảnh báo khả năng đọc không tự chặn mọi bản xuất.

**Dữ liệu và cache:** không sửa `lines.json`, `translation.json` hoặc WAV. Nếu lưu cache bố cục, có phiên bản và dấu vân tay từ chữ, mốc câu, kích thước khung và kiểu chữ. Đổi riêng bố cục chỉ làm lại phụ đề/bản có chữ cần thiết.

**Nghiệm thu:** mẫu tiếng Việt có dấu tổ hợp, tiếng Trung/Nhật/Hàn, tên Latin, số tiền, URL, câu rất dài, câu rất ngắn, khoảng nghỉ và câu chồng nhau. Kiểm tra không mất/lặp chữ; đoạn thử và bản xuất cùng bố cục. Đối chiếu hash bản dịch và WAV trước/sau. Xem video thật ở 9:16, 16:9 và 1:1 để đánh giá khả năng đọc.

**Vùng mã chính:** `pipeline/captions.rs`, `pipeline/sample.rs`, `work/fitWarning.ts`, `work/script.tsx`, các phần xuất trong `pipeline/assemble.rs`.

## Mốc B — Chỉnh mốc câu ngay cạnh video

**Kết quả người dùng nhận được:** sửa chỗ vào/ra của câu, nghe lặp đoạn và xuất lại mà không đọc lại lời không đổi.

1. Mở phần Chỉnh thời gian từ danh sách lời thoại; có ô bắt đầu/kết thúc trước, sau đó bổ sung kéo hai đầu và di chuyển câu trên timeline.
2. Hiển thị sóng âm tiếng gốc khi có nguồn phù hợp. Tạo dữ liệu biên độ nhỏ bằng FFmpeg trên máy, có giới hạn tài nguyên và cache theo nguồn; không giữ toàn bộ PCM của video dài trong bộ nhớ trình duyệt. Không phụ thuộc `audio.wav` vì file này có thể đã được dọn.
3. Bấm timeline để phát đúng đoạn; nghe lặp câu với khoảng đệm ngắn. Video đang xem phải được ghi rõ là nguồn hay bản xuất cũ: sửa nháp chưa có nghĩa âm thanh bản xuất đã đổi.
4. Xem trước thay đổi trong phiên, Hoàn tác và Hủy trước khi lưu. Kiểm tra mốc trong Rust; không chỉ tin dữ liệu giao diện.
5. Mặc định không tạo câu chồng nhau mới; câu chồng có sẵn hiện cảnh báo để người dùng quyết định sửa. Không tự thay mốc câu lân cận.
6. Mở rộng thao tác tạo lại hiện có để nhận sửa thời gian. Đổi riêng mốc giữ WAV gốc, tính lại căn tiếng, trộn và phụ đề; chỉ đọc lại câu nếu chữ/cách đọc/giọng thực sự đổi. Thuật toán căn tiếng có thể ảnh hưởng các câu cạnh nhau nên phải tính lại phần phụ thuộc, không hứa chỉ một câu thay đổi trong bản trộn.
7. Chỉ lưu khi việc tương ứng không chạy; dùng phiên bản dữ liệu để phát hiện thao tác sửa trên danh sách đã cũ. Lưu theo giao dịch và mở rộng phục hồi retake hiện có khi lỗi/hủy.

**Dữ liệu và cache:** giữ transcript Whisper làm nguồn gốc; lưu chỉnh sửa trong lớp biên tập có phiên bản, được các đường xuất/chạy tiếp cùng đọc. Không âm thầm ghi ngược lời Whisper. Trước triển khai phải rà đủ các đường đọc mốc: bản xuất, chỉ chữ, audio, đoạn thử, sửa tên và chạy tiếp. Xem trước dịch trực tiếp giữ hành vi riêng và ghi rõ giới hạn.

**Nghiệm thu:** đổi riêng thời gian không gọi Whisper/dịch/TTS; WAV giữ hash. Thử câu đầu/cuối, sát câu bên cạnh, ngoài độ dài video, NaN, câu quá ngắn, hủy/lỗi ghi, khởi động lại và video không có tiếng. Đường audio liền mạch không được giả vờ áp dụng mốc như video; chỉ bật thao tác tại chế độ có ý nghĩa và có hỗ trợ đầy đủ.

**Vùng mã chính:** `work/script.tsx`, `work/playback.ts`, `work/types.ts`, `job.rs`, `pipeline/retake.rs`, `pipeline/assemble.rs`, đăng ký IPC trong `lib.rs`.

## Mốc C — Tách/gộp câu với định danh ổn định

**Phụ thuộc:** mốc B và thiết kế dữ liệu được kiểm chứng. Đây là phần có rủi ro cache cao nhất, triển khai riêng.

1. Bổ sung ID ổn định và quan hệ tới câu nguồn trong lớp biên tập. Giữ bộ đọc tương thích việc cũ và tên WAV số thứ tự; không đổi tên toàn bộ WAV khi người dùng chỉ tách một câu.
2. Tách tại vị trí phát, cho người dùng sửa cả hai phần chữ trước khi áp dụng; không coi chia chữ theo tỉ lệ thời gian là căn từ chính xác. Gộp hai câu liền nhau với mốc và chữ được xem lại.
3. Giữ liên kết lời gốc, bản dịch và sửa tên; không tự gọi AI để đoán lời mới. Định nghĩa rõ cách xử lý khoảng nghỉ, câu rỗng và hai câu khác người nói khi chưa có mô hình nhiều người nói.
4. Chỉ bỏ cache tiếng của câu thực sự tách/gộp/đổi chữ. Các câu không liên quan vẫn nhận đúng WAV dù vị trí trong danh sách thay đổi. Tính lại bản trộn và phụ đề cần thiết.
5. Lưu nguyên tử và phục hồi được khi gián đoạn; việc cũ mở và chạy tiếp được. Chưa có định dạng tương thích thì báo rõ, không đoán ánh xạ WAV.

**Nghiệm thu:** tách/gộp đầu, giữa, cuối; thực hiện liên tiếp và hoàn tác; mở lại app; sửa tên sau tách/gộp; phát lại bản cũ. Kiểm tra hash và ánh xạ WAV không liên quan, hủy trong lúc tạo lại và việc dùng định dạng cũ.

## Mốc D — Lịch sử bản sửa dễ hiểu

1. Mỗi lần xuất sau sửa thành công lưu bản ghi gồm cấu hình, thời điểm, kết quả và lý do ngắn: đổi chữ, đổi mốc, đổi kiểu chữ hoặc âm lượng.
2. Lịch sử chỉ hiện bản đã ghi thành công, kiểm tra file còn tồn tại; bản lỗi không được hiện như bản hoàn tất.
3. Cho mở/tải bản cũ và xem cấu hình. Nạp cấu hình bản cũ để sửa tiếp là thao tác riêng, không âm thầm thay cấu hình đang chạy.
4. Không lưu API key; không nhân đôi mọi WAV cho mỗi phiên bản. Đợt đầu không cam kết tái dựng chính xác bản cũ nếu đầu vào trung gian không còn; file kết quả đã lưu vẫn mở được.
5. Giữ các folder `cu/`, `incomplete/` và bản cũ hiện có; bản thiếu metadata hiện là bản cũ không rõ thay đổi, không tự suy đoán lịch sử.

**Nghiệm thu:** hai bản khác kiểu chữ, một bản sửa lời, một lần xuất lỗi; khởi động lại vẫn mở đúng từng bản. Lưu/tải video vẫn chọn đúng `.mp4` và `.chu.mp4`; đổi phiên bản khi đang chạy bị từ chối rõ ràng.

## Hạng mục tùy chọn sau các mốc chính — Kiểm tra bản dịch có mục tiêu

Không bật thêm nhiều vòng AI mặc định. Trước hết đo lỗi trên mẫu thật; nếu số liệu, phủ định hoặc thuật ngữ là lỗi lặp lại thì bổ sung kiểm tra có mục tiêu.

- Ưu tiên kiểm tra tại máy cho số, đơn vị và thiếu lời; chỉ đánh dấu nghi vấn vì cách viết số có thể thay đổi hợp lệ khi dịch.
- Nếu dùng AI: chỉ gửi chữ ở các câu liên quan cùng ngữ cảnh cần thiết qua dịch vụ người dùng đã chọn. Có thao tác chủ động, báo số câu và không tự đổi dịch vụ.
- Gợi ý sửa phải cho xem trước, áp dụng từng câu; không coi một model kiểm tra là bằng chứng chắc chắn đúng. Chỉ tạo lại tiếng của câu người dùng chấp nhận sửa.
- Giới hạn lượt gọi, hủy được, giữ kết quả đã xong và cache theo nội dung/model/prompt. Không chạy lại toàn video chỉ vì đổi kiểu chữ.
- Chỉ triển khai sau khi so sánh chất lượng, thời gian và số lượt gọi trên cùng bộ mẫu; không hứa tiết kiệm token hoặc nâng chất lượng khi chưa đo.

## Kiểm chứng và bàn giao

- Mỗi mốc có kiểm thử dữ liệu/cơ chế cache phù hợp, build frontend và kiểm tra Rust khi thay Rust. Dùng `npm run verify:local` tại các mốc tích hợp, không chạy lặp vô ích sau thay đổi chỉ có tài liệu.
- Dùng fixture nhỏ cho lỗi định dạng, Unicode và phục hồi; dùng video thật để nghe/xem mốc câu, độ dễ đọc và phần tiếng bị ảnh hưởng. Chỉ dùng nguồn và dịch vụ có sẵn, không tự tải model hoặc mua dịch vụ.
- Bàn giao báo cáo gồm hành vi đã kiểm chứng, mẫu trước/sau và nhánh chưa thử. Không lấy ASR chép lại hoặc kiểm thử tự động làm bằng chứng duy nhất về chất lượng nghe/đọc.
- Không ấn định ngày hoàn thành trước khi chốt mô hình biên tập và đo tác động của mốc B/C lên các đường cache.

**Thứ tự đề nghị:** A → B → C → D. Đợt đầu chỉ A; dùng video thật đánh giá trước khi đầu tư timeline và thay đổi định danh. Kiểm tra dịch là tùy chọn dựa trên lỗi quan sát được.

## Tài liệu tham khảo đã đọc

Các liên kết theo nhánh `master` có thể thay đổi; lúc triển khai cần ghi commit tham chiếu. Học thiết kế, không mặc định sao chép code hay thêm dependency của repo kia.

- [OpenCreator README](https://github.com/krillinai/OpenCreator#readme)
- [Dịch và chia phụ đề](https://github.com/krillinai/OpenCreator/blob/master/runtime/krillinai/docs/paired-subtitle-translation.md)
- [Ngữ cảnh và thuật ngữ](https://github.com/krillinai/OpenCreator/blob/master/runtime/krillinai/docs/subtitle-semantic-context.md)
- [Timeline](https://github.com/krillinai/OpenCreator/blob/master/apps/web/src/features/dashboard/SubtitleTimeline.tsx)
- [Các thao tác sửa câu](https://github.com/krillinai/OpenCreator/blob/master/apps/web/src/features/dashboard/subtitle-editing.ts)
- [Thời gian hiển thị và kiểm tra phụ đề](https://github.com/krillinai/OpenCreator/blob/master/runtime/krillinai/internal/service/subtitle_display_timing.go)
- [Nguồn gốc và phiên bản kết quả](https://github.com/krillinai/OpenCreator/blob/master/docs/creator-artifact-versions.md)
