# Hướng giao diện

Ứng dụng lồng tiếng desktop: tập trung vào video, dễ chọn nguồn và chạy, dễ biết
đang làm tới đâu. Giữ bố cục thư viện bên trái, vùng làm việc bên phải.

## Màu sắc

- Nền: `#151412`; bề mặt: `#1e1d1a`; lớp nổi: `#2a2823`; ô nhập: `#171613`.
- Chữ chính: `#f3ede3`; chữ phụ: `#b3a99a`.
- Hổ phách `#e7a15a` dành cho hành động chính và điểm cần chú ý.
- Xanh cho sẵn sàng, vàng cho đang xử lý, đỏ nhạt cho lỗi hoặc thiếu công cụ.
  Trạng thái luôn có chữ giải thích, không chỉ phân biệt bằng màu.
- Dùng các biến trong `src/App.css` để giữ màu nhất quán giữa các màn.

## Chữ

- Be Vietnam Pro 400/500/600/700 cho nội dung và điều khiển, hỗ trợ tiếng Việt.
- Fraunces 600 cho tiêu đề lớn hiện có; không dùng cho đoạn dài hay ô nhập.
- Font đóng gói cùng ứng dụng, không tải font qua mạng khi mở app.
- Chữ giao diện cơ sở 14px, giãn dòng 1,55; dùng độ đậm và khoảng cách để phân cấp.

## Thao tác

- Màn tạo video ưu tiên nguồn → ngôn ngữ và giọng → tùy chọn kết quả.
- Cấu hình dịch vụ, API key và model nằm trong vùng mở rộng có tóm tắt dịch vụ.
  Khi cần nhập key, mở vùng đó để người dùng thấy việc cần làm.
- Nút Chạy là nút chính; Xem trước / Nghe trước là nút phụ cạnh nó.
- Nút chạy nằm ở chân vùng làm việc, luôn nhìn thấy khi cuộn cấu hình.
- Thư viện dùng nền chọn nhẹ, tên dài giới hạn hai dòng để quét danh sách nhanh.
- Cửa sổ hẹp dưới 701px xếp thư viện phía trên; mỗi vùng cuộn riêng.
- Có viền focus cho bàn phím, trạng thái chọn và giảm chuyển động khi hệ thống yêu cầu.

## Giới hạn

Giữ Tauri, tên sản phẩm, tiếng Việt và hành vi xử lý hiện có. Không tự đổi model,
giọng, nguồn video hoặc tùy chọn đã lưu chỉ để làm giao diện trông đẹp hơn.
