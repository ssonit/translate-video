# Bước Tên

Ngày: 2026-10-12

## Mục tiêu

Whisper hay chép sai tên. “Lina” phải thành “Lynae”, “Grock bot” phải thành “Grok bot”. Tên đúng xuất hiện cả ở phụ đề và ở giọng đọc.

Lần đầu của mỗi video, AI điền sẵn các cặp nó thấy sai. App dừng. Người dùng sửa, thêm hoặc xóa, rồi mới dịch. Lần chạy sau áp danh sách đã lưu và không dừng.

Xem trước không vào bước này.

## Đã chốt

- Bước mới `names`, nhãn **Tên**, đứng sau Ngắt câu và trước Thuật ngữ.
- Áp lên chữ trước khi dịch. Phụ đề và giọng cùng nhận chữ đã sửa.
- `lines.json` giữ nguyên lời Whisper. Danh sách nằm ở `names.json` trong folder việc.
- AI dùng đúng model đang chọn để dịch. Chỉ đề xuất cụm có trong lời. Không chắc, lỗi model, hoặc JSON hỏng thì danh sách trống và màn vẫn mở. Video không fail vì AI không đề xuất được.
- **Dịch tiếp** ghi danh sách và chạy tiếp. **Giữ nguyên** ghi danh sách rỗng đã xác nhận, không thay chữ. **Dừng** hủy cả lượt, không ghi `names.json`, lần sau hỏi lại.
- Lần sau, `confirmed: true` thì áp dụng và không dừng.
- Nút **Sửa tên** trên việc đã có. Lưu một danh sách khác thì xóa bản dịch, thuật ngữ và tiếng đã đọc. Giữ lời chép. Người dùng chạy lại.
- Bước thuật ngữ cũ giữ nguyên, vẫn tắt mặc định. Nó không sửa cách đọc.
- Không đổi tên app, `productName`, identifier, tiêu đề cửa sổ.

## Không làm

- Không danh sách tên chung trong Cài đặt.
- Không dừng Xem trước, không tự thay tên trong lúc xem.
- Không sửa `lines.json` thành tên mới. Lời gốc phải còn để đổi danh sách sau này.
- Không thay cụm ngắn hơn 2 chữ cái, ô trống, hoặc hai ô giống nhau (không phân biệt hoa thường).

## File `names.json`

```json
{
  "confirmed": true,
  "names": [
    { "from": "Lina", "to": "Lynae" },
    { "from": "Grock bot", "to": "Grok bot" }
  ]
}
```

File chưa có, hoặc `confirmed` không phải `true`, thì lần lồng đầy đủ sẽ dừng để hỏi. File hỏng thì coi như chưa có.

## Cách thay chữ

- Cụm dài được xét trước. “Grock bot” không bị cắt bởi “Grock”.
- Khớp không phân biệt hoa thường. Chữ ghi vào câu là đúng chuỗi ở `to`.
- Chỉ khớp cả cụm: ký tự ngay trước và ngay sau không phải chữ cái. “Lina” không đụng “Linaria”. “Lina,” vẫn thành “Lynae,”.
- Một lượt từ trái sang phải. Không quét lại phần vừa thay.
- Mọi lần xuất hiện trong mọi câu đều được thay.

## Luồng

`start_dub` vẫn là một lệnh. Sau ngắt câu:

1. Nếu `names.json` đã `confirmed`, báo bước xong và áp danh sách trong bộ nhớ. Không gọi model, không dừng.
2. Ngược lại, gửi khoảng 6000 ký tự đầu của lời cho model. Prompt chỉ xin JSON `{"names":[{"from":"","to":""}]}`, tối đa 20 cặp.
3. Giữ cặp có `from` xuất hiện trong cả lời (đúng luật khớp ở trên).
4. Ghi đề xuất vào snapshot của lượt chạy, phát sự kiện `names-review` với `{ names, note }`, rồi chờ.
5. Cửa sổ gọi `confirm_names` với `keep` và `names`. `keep: true` là Giữ nguyên.
6. Ghi `names.json`. Nếu bản dịch đang lưu không còn khớp lời đã thay, xóa bản dịch, thuật ngữ, wav, phụ đề và video kết quả, rồi dịch và đọc lại. Lời chép giữ nguyên.

Cửa sổ tải lại giữa lúc chờ thì `dub_status` còn đề xuất, màn Tên hiện lại.

`read_names` và `save_names` nhận folder việc. `save_names` luôn ghi `confirmed: true`. Chữ ký danh sách (from viết thường, to đúng như đã gõ, không kể thứ tự) mà đổi thì xóa `translation.json`, `terms.json`, wav từng câu, phụ đề, video kết quả và audio kết quả.

## Màn

Đúng chỗ thanh tiến độ, không thêm mục Cài đặt. Hai cột **Nghe được** và **Tên đúng**, xóa được dòng, nút **Thêm tên**. Dòng không hợp lệ thì báo và chưa gửi. AI có ghi chú (không đề xuất được, hoặc không thấy tên cần sửa) thì hiện dưới câu hướng dẫn.

Việc đã xong hoặc đang dở có nút **Sửa tên** khi không có lượt đang chạy. Cùng form, nút **Lưu** và **Huỷ**.
