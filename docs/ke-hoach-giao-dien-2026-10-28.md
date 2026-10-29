# Kế hoạch nâng cấp giao diện desktop

Ngày: 28/10/2026. Trạng thái: đã triển khai khung điều hướng, hệ màu và các màn chính. Chi tiết kết quả, kiểm chứng và các phần còn cần nghiệm thu: [Giao diện mới](giao-dien-moi-2026-10-28.md). Các nội dung dưới đây là định hướng thiết kế, không phải tuyên bố mọi tiêu chí đã được nghiệm thu.

## 1. Mục tiêu và phạm vi

Chuyển từ bố cục vài khung đa năng sang các không gian làm việc rõ ràng. Người mới vẫn chọn video và chạy nhanh; người chỉnh sửa có đủ chỗ cho video, lời thoại và timeline. Giữ toàn bộ chức năng hiện có, Tauri 2 + React + Rust, pipeline và dữ liệu việc cũ.

Giữ tiếng Việt hiện tại và font Be Vietnam Pro; thiết kế lại bảng màu theo cách tổ chức màu của repo tham khảo `D:/Project/folia-major`: nền trung tính có phân lớp, màu chữ rõ vai trò, điểm nhấn tiết chế và có kiểm tra tương phản. Hướng đề xuất là than trung tính + cam, thay nền nâu/cam phủ rộng hiện tại. Không đổi tên sản phẩm, identifier, tên gói hoặc tiêu đề cửa sổ trong đợt này. Không thêm nhân bản giọng, phân vai người nói hay tính năng pipeline mới.

## 2. Vấn đề xác nhận từ mã hiện tại

- `App.tsx` chuyển giữa Đồ nghề và Work; Work được giữ mounted để không mất việc đang chạy.
- `Work.tsx` vừa điều phối việc vừa chuyển nhiều pane theo `s.view`; điều hướng và trạng thái xử lý đang gắn với nhau.
- `.stage-body` trong `App.css` dành khoảng 270–320 px cho thư viện. Cửa sổ mặc định 1080 × 760 còn ít chỗ cho công cụ chỉnh sửa.
- `WatchPane` ghép phát kết quả, thao tác xuất, lịch sử và lời thoại. `TimingEditor` nằm trong khu lời thoại, panel giới hạn 24rem và tự cuộn.
- `StyleEditor` và `ProductionOptions` chứa nhiều nhóm chuyên sâu trong các khối mở rộng; khó nhìn tổng thể cấu hình đang áp dụng.
- Có nhiều lớp breakpoint trong CSS. Thêm quy tắc chồng lên tiếp sẽ khó dự đoán bố cục.

Hệ quả: nút dài bị chen, công cụ quan trọng ở dưới vùng cuộn, timeline thiếu chiều ngang và bản đang phát dễ bị nhầm với nháp đang sửa.

## 3. Kiến trúc điều hướng đề xuất

Thanh điều hướng trái chỉ dành cho điều hướng, không ghim toàn bộ danh sách video vào mọi màn. Có thể thu gọn nhưng mỗi biểu tượng phải có nhãn hoặc tooltip.

| Điểm đến | Nhiệm vụ | Nội dung chính |
| --- | --- | --- |
| Lồng tiếng | Tạo việc mới, là màn mở app mặc định | Nguồn, kết quả, ngôn ngữ/giọng, dịch vụ dịch, bộ cài đặt, tóm tắt và chạy |
| Hàng đợi | Theo dõi và điều phối xử lý | Đang chạy, chờ duyệt tên, việc chờ, thử lại, thứ tự và chính sách khi lỗi |
| Thư viện | Tìm và mở việc đã có | Tìm kiếm; lọc Tất cả / Đang dở / Đã xong; thumbnail, nguồn, cập nhật gần nhất |
| Bộ cài đặt | Quản lý cấu hình tái sử dụng | Danh sách bộ, xem cấu hình, sửa tên bộ, lưu và xóa bằng các khả năng hiện có |
| Cài đặt | Thiết lập dùng chung | Nơi lưu mặc định, dịch vụ/key, giọng mặc định, tùy chọn xử lý và Đồ nghề |

**Chỉnh sửa** là màn chi tiết mở từ một video trong Thư viện hoặc kết quả đã xong, không phải một mục điều hướng trống cần người dùng tự tìm video lần nữa. Có đường quay về danh sách và giữ nguyên bộ lọc/vị trí trước đó.

Chip kiểm tra máy vẫn ở đầu màn Lồng tiếng, mở trực tiếp Đồ nghề. Đồ nghề luôn có đường quay lại Lồng tiếng. Thanh trạng thái việc đang chạy hiện trên các màn liên quan, bấm để tới đúng việc trong Hàng đợi.

## 4. Các màn và hành vi cụ thể

### 4.1. Lồng tiếng

Một vùng nội dung thoáng, không có danh sách thư viện cố định bên cạnh:

1. Chọn file hoặc dán link; tên nguồn rõ ràng, cho đổi nguồn.
2. Chọn Video lồng tiếng / Audio / Video phụ đề.
3. Chọn ngôn ngữ; chỉ hiện giọng khi kết quả cần đọc tiếng.
4. Dịch bằng: tên dịch vụ đang chọn và thao tác đổi nhanh; key/model chi tiết ở Cài đặt, có đường mở đúng mục và quay lại giữ bản nháp.
5. Bộ cài đặt và tóm tắt hình/chữ/tiếng; có nút mở trình cấu hình rộng khi cần chỉnh sâu trước khi chạy.
6. Nơi lưu thực tế và hành động Chạy / Thêm hàng đợi / Xem trước hoặc Nghe trước theo chế độ.

Giữ khả năng đổi Không dịch và Bật/Tắt DeepSeek local tại đây. Thay lựa chọn phải kiểm tra đồ nghề lại như hiện nay. Thiếu mục bắt buộc phải có lý do và nút Xem đồ nghề cạnh hành động bị khóa.

Không chia quy trình tạo việc thành wizard bắt buộc nhiều bước. Người dùng quen vẫn chạy một video từ một màn.

### 4.2. Hàng đợi

- Mỗi dòng có tên/thumbnail nguồn, trạng thái bằng chữ, cấu hình tóm tắt và tiến độ.
- Việc đang chạy có một vùng chi tiết: bốn giai đoạn mặc định; chín bước và lỗi kỹ thuật mở khi cần.
- Chờ duyệt tên có hành động Kiểm tra tên nổi bật; mở vùng duyệt đủ rộng rồi xác nhận dịch tiếp.
- Giữ đủ chọn nhiều, áp bộ, đổi thứ tự, bỏ qua, thử lại, lịch sử và chính sách khi lỗi.
- Chuyển màn không dừng xử lý; cấu hình mỗi việc là bản đã chụp khi thêm, không bị Cài đặt hiện tại ghi đè.
- Dừng và Bỏ qua ghi rõ phạm vi; trạng thái chờ phản hồi được thể hiện ngay để tránh bấm lặp.

### 4.3. Thư viện

- Video có tên dài vẫn đọc được; hỗ trợ nguồn local và link, không chỉ dùng tên folder việc.
- Việc dở hiển thị tiến độ nội dung, thời điểm cập nhật và vài câu đầu từ `draft_brief`; mở ra có video nguồn và Chạy tiếp.
- Việc hoàn tất mở màn xem/chỉnh sửa của video đó.
- Lịch sử bản xuất nằm trong chi tiết video, không thành một thư viện toàn cục thứ hai.
- Trạng thái trống, đang đọc, lỗi đọc và nguồn không còn tồn tại có hướng xử lý riêng.
- Bộ lọc ban đầu dùng dữ liệu hiện có; không thêm cơ sở dữ liệu hoặc quét mạng cho thumbnail.

### 4.4. Không gian chỉnh sửa video

Khung tham chiếu ở cửa sổ rộng:

```text
Quay lại · Tên video       Bản đang xem · Trạng thái nháp
--------------------------------------------------------
                         | Lời thoại / Hình & chữ /
       Video             | Âm thanh / Bản xuất
                         | Công cụ của mục đang chọn
--------------------------------------------------------
Phát / Lặp · Vị trí phát · Thu phóng · Hoàn tác
Sóng âm + các câu, chạy hết chiều ngang vùng làm việc
--------------------------------------------------------
Thay đổi chưa xuất · Tóm tắt             Xuất lại
```

- Video có tỉ lệ đúng, không bị ép nhỏ để nhường chỗ cho mọi nhóm công cụ cùng lúc.
- Tab **Lời thoại**: lời gốc/lời dịch, sửa chữ, tạo lại câu, cảnh báo cần kiểm tra; chọn câu đồng bộ vùng timeline.
- Timeline là vùng riêng dưới video và bảng công cụ, không lồng trong danh sách lời thoại. Có thể thu gọn khi chỉ xem.
- Mốc bắt đầu/kết thúc và thao tác tách/gộp hiện theo câu đang chọn. Giữ nghe lặp, hoàn tác mốc và hoàn tác tách/gộp đúng hai phạm vi hiện có.
- Tab **Hình & chữ**: khung, crop/nền mờ, kiểu chữ, vị trí, logo và vùng che; giữ preview kéo trực tiếp.
- Tab **Âm thanh**: mức tiếng gốc/tiếng lồng, tự hạ tiếng gốc, cách đọc tên và nghe thử. Phân biệt sửa tên nội dung với cách phát âm.
- Tab **Bản xuất**: lịch sử phiên bản, mở/tải bản đang chọn, xem và nạp cấu hình. Nạp cấu hình vẫn là hành động riêng có mô tả dùng lời/mốc hiện tại.
- Dựng thử 8 giây có vị trí rõ trong công cụ xuất; chỉ bật khi đủ cache, không lẫn với Xem trước dịch trực tiếp.
- Thanh Xuất lại luôn nhìn thấy; ghi rõ bản đang phát chưa phản ánh nháp. Thành công mới chuyển sang bản mới, lỗi/hủy giữ ngữ cảnh để sửa tiếp.
- Có thể kéo thay đổi độ rộng hai vùng và chiều cao timeline trong giới hạn hợp lý; dùng bàn phím vẫn thao tác được.
- Audio có bố cục riêng: trình phát audio và lời thoại rộng, không để chỗ trống cho khung hình. Audio liền mạch không hiện chỉnh mốc mà backend không hỗ trợ.

Trước khi xử lý lần đầu, trình cấu hình chỉ mở các mục hình/chữ/tiếng có sẵn; chưa hiện timeline giả hoặc công cụ cần transcript. Việc chọn nguồn xong cấu hình và việc sửa kết quả dùng chung control, nhưng có ngữ cảnh và điều kiện bật khác nhau.

### 4.5. Bộ cài đặt và Cài đặt

- Bộ cài đặt có trang quản lý riêng; chọn nhanh vẫn nằm tại màn tạo việc. Chỉnh bộ không tự cập nhật việc đã thêm.
- Cài đặt chia nhóm: Dịch vụ dịch; Giọng; Lưu trữ & mặc định xử lý; Đồ nghề.
- Một nguồn trạng thái cho lựa chọn dịch vụ/key và giọng; sửa từ nơi nào cũng phản ánh nhất quán.
- Phân biệt mặc định dùng chung, cấu hình video đang mở và cấu hình chụp trong hàng đợi. Trên màn có nhãn phạm vi trước khi lưu/áp dụng.
- Đồ nghề giữ trạng thái chưa có kết quả là Đang kiểm tra, giữ ngoại lệ tải Whisper, không tự tải phần mềm/model/voice khác.

## 5. Quy chuẩn bố cục và thao tác

- Khoảng cách theo thang 4/8/12/16/24/32 px; tránh mỗi component tự đặt một nhịp.
- Chữ nội dung khoảng 14–16 px; chữ phụ không dùng cho thông tin cần thao tác hoặc cảnh báo quan trọng.
- Button/input thông thường cao 40–44 px; thao tác compact trong danh sách có thể thấp hơn nhưng vùng bấm tối thiểu 32 px.
- Nút cam đặc cho hành động chính của vùng; nút phụ trung tính, nút nguy hiểm có chữ rõ. Bỏ kiểu nút viên thuốc cho mọi thao tác; dùng bo góc vừa, nhất quán.
- Nhãn ngắn nhưng rõ: Nghe lặp, Lưu mốc nháp, Xuất lại. Tooltip bổ sung ý nghĩa, không thay toàn bộ nhãn thao tác quan trọng.
- Bản nháp chưa lưu / đã lưu nháp / đang xuất / đã xuất / lỗi cần có chữ, không chỉ đổi màu.
- Focus bàn phím rõ; thứ tự Tab hợp lý; nút biểu tượng có accessible name; phím tắt không kích hoạt khi đang gõ.
- Không cuộn lồng trong form chỉnh sửa thông thường. Danh sách dài được cuộn độc lập, thanh hành động không trôi mất.
- Có loading/empty/error/disabled với nguyên nhân cụ thể; không tự suy đoán thành thiếu công cụ khi IPC thất bại.

### Kích thước cần kiểm chứng

| Vùng hiển thị theo CSS px | Hành vi dự kiến |
| --- | --- |
| Từ 1440 px | Navigation có nhãn, video và công cụ cạnh nhau, timeline rộng |
| 1080–1439 px | Navigation gọn; công cụ vẫn có chiều rộng tối thiểu, cho thu thanh bên |
| 860–1079 px | Navigation thu gọn; công cụ thành tab/vùng chuyển đổi thay vì ép hai cột |
| Dưới 860 px | Một vùng chính mỗi lần; giữ truy cập video, công cụ và Xuất lại; tránh cuộn ngang toàn cửa sổ |

Các mốc là giả thuyết thiết kế, chốt sau wireframe. Kiểm tra thực tế 1080 × 760 hiện tại, màn 1366 × 768, 1920 × 1080 và Windows scale 125%/150%. Không giải quyết bằng cách bắt người dùng phóng to cửa sổ.

### Hệ màu chủ đạo — học từ Folia

Đã đọc mã tham khảo local, không sửa repo Folia:

- `D:/Project/folia-major/src/services/baseThemes.ts`: theme tối mặc định dùng nền `#09090B`, chữ/nhấn `#F4F4F5`, chữ phụ `#71717A`; theme sáng dùng nền `#F5F5F4`, chữ `#1C1917`, nhấn cam `#EA580C`, chữ phụ `#44403C`.
- `src/utils/builtinTheme/generateBuiltinDualTheme.ts`: phối màu theo vai trò và sắc độ; điều chỉnh độ sáng để đạt tương phản thay vì chỉ chọn các màu nhìn hợp nhau.
- `src/utils/builtinTheme/themeColorRanges.ts`: bộ sinh theme đặt ngưỡng tương phản chữ chính 9:1, chữ phụ 4.5:1 và accent 3.2:1 trên nền. Đây là ngưỡng của bộ sinh theme, không đồng nghĩa mọi tổ hợp control thực tế đã đạt chuẩn.

App hiện tại tự khai báo palette trong `src/App.css` (`#151412`, `#1E1D1A`, accent `#E7A15A`); không có căn cứ coi palette này là mặc định Tauri. Điểm cần cải thiện là sắc nâu phủ nhiều lớp và vai trò màu chưa tạo phân cấp thao tác đủ rõ.

**Palette đề xuất cho app này**: nền than trung tính, chữ trắng dịu, nhấn cam. Các mã dưới là đề xuất riêng, không phải chép nguyên theme tối mặc định của Folia.

| Token / vai trò | Màu dự kiến | Cách dùng |
| --- | --- | --- |
| `canvas` | `#09090B` | Nền ngoài và vùng trống quanh video |
| `surface` | `#121215` | Navigation, nền vùng công cụ |
| `surface-raised` | `#1C1C21` | Menu, hộp thoại, control và nhóm nổi |
| `surface-hover` | `#27272E` | Hover trung tính, không tô cam mọi nút |
| `border` | `#34343D` | Phân tách trang trí; không dùng làm dấu hiệu duy nhất của control cần nhận biết |
| `control-border` | `#71717A` | Viền ô nhập/điều khiển khi cần phân biệt rõ với nền |
| `text` | `#F4F4F5` | Nội dung chính |
| `text-secondary` | `#A1A1AA` | Nhãn phụ cần đọc, thời gian, thông tin nguồn |
| `accent` | `#F97316` | Nút Chạy/Xuất lại, focus, tiến độ và câu đang chọn |
| `accent-hover` | `#FB923C` | Hover của nút chính |
| `on-accent` | `#18120C` | Chữ tối trên nút cam, tránh chữ trắng thiếu tương phản |
| `accent-soft` | Accent pha khoảng 12–16% trên surface | Nền mục chọn, câu chọn; chữ/viền vẫn đủ rõ |
| `success` | `#86D6A0` | Đã xong, đủ đồ nghề |
| `warning` | `#F2CB72` | Cần kiểm tra, chờ duyệt, cảnh báo mốc |
| `danger` | `#F28B91` | Lỗi và thao tác nguy hiểm |

Quy tắc áp dụng:

1. Nền trung tính chiếm phần lớn diện tích. Cam tập trung vào hành động chính và lựa chọn hiện hành; không dùng cam trang trí mọi viền, mọi icon và toàn bộ sóng âm.
2. Timeline dùng sóng âm xám đủ thấy; câu được chọn dùng cam, câu khác dùng nền/viền trung tính. Playhead có tương phản rõ, cảnh báo có biểu tượng/chữ riêng.
3. Tách `accent` nhận diện khỏi màu trạng thái. Chờ duyệt không trông như lỗi; nút dừng không cạnh tranh với nút xuất bằng cùng một kiểu cam đặc.
4. Bớt gradient nâu/cam và viền pill bao quanh các nhóm. Tạo chiều sâu bằng surface, spacing và một số đường phân tách nhẹ.
5. Kiểm tra chữ thường tối thiểu 4.5:1 trên đúng nền thực tế, chữ lớn và dấu hiệu điều khiển cần thiết tối thiểu 3:1. Đo cả hover, selected, focus và nền pha alpha; không chỉ đo mã màu gốc.
6. Chữ phụ cần đọc không lấy nguyên `#71717A` của Folia trên mọi nền. `#A1A1AA` là đề xuất để giữ độ rõ trong công cụ dày thông tin; vẫn đo trên từng surface.
7. Định nghĩa palette gốc và token theo ngữ nghĩa riêng; component dùng token, không chèn hex tùy ý. Disabled có phong cách riêng và vẫn giữ lý do khóa dễ đọc bên cạnh.
8. Màu ứng dụng ổn định khi đổi video; không tự lấy màu thumbnail làm theme vì công cụ cần nhận diện nhất quán. Không đưa hệ sinh theme/AI theme của Folia vào phạm vi này.

Theme sáng là hướng có thể bổ sung sau với nền ngà/cam theo Folia. Đợt này ưu tiên hoàn thiện một theme tối; chưa thêm bộ chọn nhiều theme hoặc cơ chế đổi tự động.

Đầu ra đợt thiết kế cần có bảng swatch và mẫu thật cho navigation, button, input, thông báo, hàng đợi và timeline trên cùng một màn; kiểm tra độ tương phản trước khi chốt token. Màu trên phụ đề xuất ra video vẫn theo lựa chọn render của người dùng, độc lập với màu giao diện app.

## 6. Hướng tổ chức mã

- Thêm app shell và điều hướng riêng; màn nào hiện không quyết định job/TTS còn sống hay không.
- Tách dần `useWorkState`, effects và actions thành trạng thái việc đang chạy, bản nháp tạo việc, video đang mở và trạng thái UI. Làm từng phần, không viết lại toàn bộ store một lượt.
- Giữ `useTtsBridge` ở gốc App, listeners và worker độc lập với chuyển màn. Không remount chúng theo route/tab.
- Tách trạng thái chọn video/phiên bản khỏi lần tải dữ liệu; tiếp tục chống phản hồi IPC cũ đến sau khi đã đổi video.
- Rà effects cleanup trước khi tháo component: `ProductionOptions` hiện có thể gọi hủy ở unmount khi dựng mẫu; chuyển màn cần chính sách rõ, không vô tình hủy job khác.
- Dùng lại logic `job`, `playback`, timing/structure, queue và Rust command hiện có. Chỉ đổi hợp đồng khi có nhu cầu đã xác nhận và kiểm thử tương thích.
- Tách CSS nền tảng (màu, chữ, spacing, control) khỏi CSS shell/màn/component; xóa quy tắc cũ sau khi chuyển màn tương ứng, tránh chồng breakpoint.
- Chọn cơ chế route nhẹ phù hợp app desktop sau khi kiểm kê trạng thái. Không bắt buộc thêm thư viện/store/router mới chỉ để thay bố cục.
- Nếu lưu kích thước panel hoặc tab, lưu riêng cấu hình UI, không trộn với metadata quyết định cache pipeline.

## 7. Thứ tự triển khai và đầu ra

| Đợt | Công việc | Điều kiện hoàn tất |
| --- | --- | --- |
| 1. Chốt cấu trúc | Lập bản đồ chức năng → màn mới; wireframe Lồng tiếng, Chỉnh sửa, Hàng đợi và biến thể cửa sổ hẹp; định nghĩa control/màu/chữ | Mọi chức năng có nơi đến; duyệt được luồng và kích thước trước khi thay màn chính |
| 2. Nền điều hướng | App shell, control dùng chung, trạng thái chạy độc lập; tách Thư viện khỏi cột cố định | Chuyển các màn khi đang xử lý/TTS không mất trạng thái hoặc hủy ngoài ý muốn |
| 3. Màn chỉnh sửa | Chuyển video/lời thoại/timeline trước; tiếp theo hình/chữ, âm thanh và phiên bản; thanh xuất cố định | Ở 1080 × 760 thao tác đầy đủ, timeline rộng, không mất nháp khi đổi tab, xuất lại đúng |
| 4. Tạo việc & hàng đợi | Làm gọn composer, trình cấu hình trước chạy; tiến độ, duyệt tên, thao tác nhiều việc | Tạo một việc nhanh; cấu hình từng việc giữ đúng; chạy tiếp/dừng/thử lại hoạt động |
| 5. Thư viện & cấu hình | Hoàn thiện lọc/tìm, việc dở, lịch sử; Bộ cài đặt và Cài đặt/Đồ nghề | Không có chức năng cũ bị bỏ sót; phạm vi lưu rõ; dữ liệu cũ vẫn mở được |
| 6. Nghiệm thu | Kiểm tra bố cục, bàn phím, scale Windows và các luồng hồi quy; dọn CSS/pane cũ; cập nhật README | Đạt checklist bên dưới và không còn đường điều hướng trùng gây nhầm |

Ưu tiên thiết kế màn chỉnh sửa trong đợt 1; khi code thì làm nền điều hướng tối thiểu trước, rồi chuyển ngay màn chỉnh sửa. Không dùng một đợt dài chỉ đổi màu/bo góc toàn app mà chưa giải quyết độ chật.

Mỗi đợt giữ app chạy được và có điểm nghiệm thu riêng. Chỉ bỏ pane cũ sau khi màn thay thế đã đủ hành vi. Chưa ước lượng lịch cố định trước khi chốt wireframe và mức tách trạng thái cần làm.

## 8. Checklist nghiệm thu

### Sử dụng và bố cục

- Chọn nguồn → chạy, mở việc cũ → chỉnh → xuất, và chờ duyệt tên → dịch tiếp đều có đường đi rõ.
- Tại 1080 × 760, nút chính và lý do khóa luôn tìm thấy; nhãn tiếng Việt dài không đè nhau hoặc bị cắt mất nghĩa.
- Không phải mở lịch sử hay rời video để sửa một mốc; kéo đầu/cuối câu, nhập số và nghe lặp đều dùng được.
- Video dọc, ngang, audio và audio liền mạch có bố cục đúng; nội dung dài không làm nút tràn.
- Đổi tab giữ nháp; rời video có cơ chế xử lý thay đổi chưa lưu rõ ràng, phù hợp cách lưu hiện tại. Không gán nhãn tự lưu khi chưa ghi dữ liệu thành công.
- Hoàn tác mốc và hoàn tác tách/gộp có nhãn phạm vi; bản đang xem luôn phân biệt được với cấu hình đã nạp để sửa.

### Luồng và tương thích

- Job/TTS tiếp tục khi đổi màn; dừng, hủy, lỗi và thử lại không tạo kết quả thành công giả.
- Thiếu công cụ bắt buộc khóa đúng nút; đang kiểm tra/lỗi đọc không bị khóa nhầm; đổi dịch vụ/key/Không dịch/DeepSeek kiểm tra lại.
- Xem trước dịch trực tiếp, Nghe trước theo phần và đoạn thử bản xuất không bị nhập làm một luồng.
- Việc dở, dữ liệu việc cũ, cache theo ngôn ngữ, bật Không dịch và nguồn khác trùng tên vẫn tuân theo hành vi hiện có.
- Sửa lời, tên, cách đọc, mốc, tách/gộp, hình/chữ và âm lượng giữ đúng phạm vi vô hiệu cache; thay giao diện không gọi AI/TTS lại ngoài ý muốn.
- Lịch sử: mở/tải đúng bản đang chọn, nạp cấu hình riêng, khóa các hành động đổi bản khi cần; video/audio và bản có chữ đều đúng.
- Hàng đợi giữ cấu hình riêng, thứ tự, chính sách lỗi và thời gian chờ duyệt tên.
- Đổi video liên tiếp trong lúc đang tải không để dữ liệu video trước ghi đè video mới.

### Xác minh kỹ thuật

- Chạy `npm run build` và các kiểm tra logic liên quan phần sửa; dùng `npm run verify:local` ở mốc tích hợp đầy đủ.
- Bổ sung kiểm tra có ý nghĩa cho chuyển màn khi đang chạy, bảo vệ nháp, điều kiện hiển thị/hành động và phản hồi tải sai thứ tự. Không viết test chỉ để chụp lại class CSS.
- Mở bằng `npm run tauri dev` để kiểm tra IPC, media, hộp thoại lưu và TTS thật; Vite riêng không đủ chứng minh.
- Chụp các màn chính ở kích thước mục tiêu, kiểm tra Windows scaling, focus bàn phím và thao tác kéo timeline.

## 9. Quyết định đề xuất để bắt đầu

Chọn nâng cấp cấu trúc theo kế hoạch này; bắt đầu bằng wireframe có đủ trạng thái của màn Lồng tiếng và Chỉnh sửa, kèm navigation tổng thể và palette than trung tính/cam học từ cách phân vai màu của Folia. Giữ tên sản phẩm và năng lực xử lý hiện tại. Chốt bố cục và màu trên mẫu control cụ thể trước khi chuyển các pane đang hoạt động, không mở rộng sang thêm tính năng xử lý video trong cùng đợt.
