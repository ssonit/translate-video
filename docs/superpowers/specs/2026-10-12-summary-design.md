# Tóm tắt sau khi xong

Ngày: 2026-10-12

## Mục tiêu

Khi video hoặc audio đã lồng xong, người dùng bấm thì được một bản tóm tắt ngắn: một đoạn mở và vài bài học cốt lõi, mỗi bài có mốc giờ để tua tới đúng chỗ trên bản kết quả.

Lúc bấm **Chạy** hoặc **Xem trước**, app không tóm tắt. Chưa bấm thì không gọi model, không thêm thời gian vào pipeline.

## Đã chốt

- Nút **Tóm tắt** nằm trên màn kết quả (`view === "watch"`), cạnh video hoặc audio đã xong. Không có nút này trên màn chuẩn bị, form chạy mới, hay lúc đang xem dần.
- Nguồn chữ là `transcript.json` đã có trong folder việc. Không chạy lại Whisper, không tách tiếng lại, không đọc giọng, không ghép lại.
- Ngôn ngữ bản tóm tắt là ngôn ngữ đã lưu trong `settings.json` của folder việc (`target`), qua `clean_target`. Không có file đó thì dùng tiếng Việt. Không thêm ô chọn ngôn ngữ.
- Một đoạn mở, hai hoặc ba câu. Năm bài học, ít hơn nếu bản ghi ngắn, không bao giờ quá bảy. Mỗi bài: tiêu đề ngắn, một hoặc hai câu, một mốc giờ.
- Bấm một bài thì tua bản đang phát (`video` hoặc `audio` trên màn kết quả) tới mốc đó, cùng cách `DubScript` gán `currentTime`.
- Ghi `summary.json` và `summary.txt` trong folder việc. Mở lại kết quả thì hiện bản đã ghi, không gọi model. **Tóm tắt lại** mới gọi lại.
- Model là lựa chọn dịch hiện tại: API key Gemini, ChatGPT, Claude, DeepSeek, hoặc Ollama. Chỉ chữ lời thoại đi mạng khi người dùng đã chọn API. Không gửi audio hay video.

## Không làm

- Không thêm bước vào `start_dub` hay `start_watch`. Tám bước lồng tiếng giữ nguyên.
- Không tóm tắt trong lúc đang lồng hoặc đang ghi chữ vào video. Nút tắt khi `running` hoặc `captioning`.
- Không dùng `translation.json` làm nguồn. Lời gốc trong `transcript.json` mới là nội dung cần tóm; ngôn ngữ đích chỉ là ngôn ngữ để viết.
- Không quiz, không câu hỏi thường gặp, không kể lại từng đoạn.
- Không đổi tên app, `productName`, identifier, tiêu đề cửa sổ.
- Không tải thêm gì, không thêm nhà cung cấp model.

## File

Folder việc là thư mục cha của file kết quả, cùng chỗ `dub_lines` đọc `translation.json`.

| Đường dẫn | Khi nào có |
|---|---|
| `transcript.json` | Đã có từ bước chép lời. Tóm tắt chỉ đọc. |
| `settings.json` | Đã có từ lượt lồng. Đọc `target`. |
| `summary.json` | Sau một lượt tóm tắt hợp lệ. App đọc file này khi mở kết quả. |
| `summary.txt` | Ghi cùng lúc với `summary.json`, để mở bằng tay. |

`summary.json`:

```json
{
  "target": "vi",
  "overview": "Hai hoặc ba câu.",
  "lessons": [
    { "start": 12.4, "title": "Tiêu đề", "text": "Một hoặc hai câu." }
  ]
}
```

`start` là giây, lấy từ `start` của một câu trong `transcript.json`, không phải số model tự bịa.

`summary.txt`, mốc giờ cùng hàm `clock` trên màn kết quả (`phút:giây`, phút có thể lớn hơn 59):

```text
{overview}

1. {phút:giây} — {title}
{text}
```

Ghi `summary.json` trước, rồi `summary.txt`. Chỉ ghi khi bản đã qua kiểm tra. Lỗi hoặc bấm Dừng thì không ghi đè file cũ. App chỉ đọc JSON. Mở kết quả mà có JSON nhưng thiếu TXT thì ghi lại TXT từ JSON, không gọi model.

Khi `drop_stale` xóa bản dịch vì đổi ngôn ngữ đích hoặc bật tắt bỏ qua dịch, xóa luôn `summary.json` và `summary.txt`. Đổi mỗi giọng đọc thì giữ bản tóm tắt.

## Luồng

1. Màn kết quả mở. Nếu `summary.json` có `target` trùng ngôn ngữ của folder việc và có ít nhất một bài học, hiện ngay.
2. Chưa có bản, hoặc bấm **Tóm tắt lại**: lệnh `summarize_result`.
3. Rust từ chối nếu đang lồng hoặc đang ghi chữ. Từ chối nếu không có `transcript.json` hoặc file rỗng, câu báo: `Chưa có lời thoại để tóm tắt.`
4. Dựng dòng thời gian từ mỗi câu: `{start} {text}`, `start` là giây với một chữ số thập phân.
5. Gọi `ollama::complete` với lựa chọn dịch hiện tại. Video ngắn, cả dòng thời gian không quá 6000 ký tự: một lần gọi, prompt ở dưới.
6. Dài hơn 6000 ký tự: cắt theo dòng, mỗi phần tối đa 6000 ký tự. Mỗi phần trả về tối đa bốn bài học ứng viên, không viết đoạn mở. Một lần gọi cuối nhận các ứng viên đó (không nhận lại cả lời thoại) và viết đoạn mở cùng năm đến bảy bài.
7. Kiểm tra JSON, gán mốc giờ, ghi hai file, trả về cho cửa sổ.
8. **Dừng** gọi `cancel_summary`, cờ riêng, không đụng cờ của `cancel_dub`. `complete` đang chạy thấy cờ thì dừng và không ghi file.

## Prompt và kiểm tra

Prompt một lần (và lần gọi cuối của bản dài), tiếng Việt, giống kiểu `summary_prompt`:

- Viết `overview` và mọi `title`, `text` bằng tên ngôn ngữ đích (`tiếng Việt`, …).
- `overview` là hai hoặc ba câu, nói bản ghi nói về việc gì.
- Năm bài học cốt lõi. Bản ghi ngắn thì ít hơn, không quá bảy.
- Mỗi bài một ý, không kể lại lời thoại. `title` vài từ. `text` một hoặc hai câu.
- `start` là số giây có sẵn trong dòng thời gian, chọn câu nói rõ ý đó nhất.
- Chỉ trả về JSON: `{"overview":"","lessons":[{"start":0,"title":"","text":""}]}`.

Prompt từng phần của bản dài chỉ xin tối đa bốn bài, cùng dạng JSON nhưng không có `overview`.

Sau khi parse:

- Bỏ bài thiếu `title` hoặc `text`.
- `title` dài hơn 80 ký tự, `text` dài hơn 320, hoặc `overview` dài hơn 500: gọi lại một lần với cùng prompt cộng câu `Ngắn hơn.` Vẫn dài thì cắt ở giới hạn đó.
- Gán `start` của model vào câu trong transcript có khoảng `[start, end)` chứa nó. Không có câu nào chứa thì lấy câu có `start` gần nhất. Mốc lưu lại là `start` của câu đó.
- Sắp các bài theo thời gian. Trùng cùng một câu thì giữ bài đứng trước.
- Còn hơn bảy bài: sau khi đã sắp, lấy bảy chỉ số `round(i * (n - 1) / 6)` với `i` từ 0 đến 6. Cách này luôn giữ bài đầu và bài cuối.
- Còn không bài, hoặc `overview` rỗng: lỗi `Không tóm tắt được.` Không ghi file.
- JSON hỏng đã được `complete` thử lại một lần. Vẫn hỏng thì cùng câu lỗi trên.

## Cửa sổ

Trong `result-actions` của màn kết quả:

- Chưa có bản: nút **Tóm tắt**.
- Đang chạy: chữ **Đang tóm tắt…** và nút **Dừng**.
- Đã có bản: đoạn mở, rồi danh sách bài. Mỗi dòng có mốc giờ, tiêu đề, câu bài học. Nút **Tóm tắt lại**.
- Không có `transcript.json`: nút tắt, dòng `Chưa có lời thoại để tóm tắt.`
- Lỗi: dòng lỗi dưới nút, bản cũ nếu có thì vẫn hiện.

Bấm một bài khi phần tử phát đã gắn: `currentTime` bằng `start`, rồi `play()`. Chưa gắn được phần tử thì không tua, danh sách vẫn hiện.

Work đã giữ trạng thái khi đổi màn. Lệnh đang chạy xong vẫn ghi file; quay lại màn kết quả thì đọc `summary.json`.

## Lệnh

- `summarize_result { path, redo }` trả về đúng dạng `summary.json`. `redo` false và file đã khớp ngôn ngữ thì trả file, không gọi model. `path` là file kết quả đang xem.
- `cancel_summary` bật cờ dừng của lượt tóm tắt.

Cả hai đăng ký cạnh `dub_lines`. Phần gọi model chạy trong `spawn_blocking`, cùng kiểu `burn_captions`.

Mã prompt, cắt phần, parse và gán mốc nằm ở `src-tauri/src/summary.rs`. `pipeline.rs` chỉ thêm hai đường dẫn vào `drop_stale`.

## Kiểm tra

Unit test trong `summary.rs`:

- Prompt có tên ngôn ngữ đích và yêu cầu JSON với `overview` cùng `lessons`.
- Dòng thời gian cắt đúng 6000 ký tự, không cắt giữa dòng.
- JSON đủ bài thì giữ; hơn bảy bài thì còn bảy, có bài đầu và bài cuối.
- Mốc nằm trong một câu thì thành `start` của câu đó; mốc ngoài mọi câu thì thành câu gần nhất.
- Bài thiếu chữ bị bỏ; hết bài thì lỗi và không cần ghi file.
- `summary.json` cùng `target` được nhận lại; khác `target` thì không.

Không cần video thật. `npm run build` không bắt buộc cho riêng phần Rust; test Rust của module này phải qua.

## Ghi cho người làm sau

Thêm một đoạn ngắn vào `AGENTS.md`: tóm tắt là nút trên màn kết quả, đọc `transcript.json`, ghi `summary.json`, không nằm trong pipeline tám bước.
