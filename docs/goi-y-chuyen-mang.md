# Gợi ý những phần có thể chuyển qua mạng

Ghi lại để làm sau. App hiện giữ tiếng và hình trên máy. Ba mục dưới đây là những chỗ chuyển đi thì máy nhẹ, kèm cái phải đánh đổi.

Đã làm trên máy, chưa đụng mạng:

- Chép lời dùng 2 luồng. Trước đó Whisper lấy từ 4 đến 8 luồng nên một video cũng làm máy đứng.
- Whisper, FFmpeg và `piper.exe` chạy ở mức ưu tiên thấp hơn cửa sổ, để máy còn dùng được trong lúc chờ.
- Bước ghi chữ vào video (`libx264`) cũng giới hạn 2 luồng.

## 1. Chép lời

Đây là phần nặng nhất còn lại trên máy (`whisper-cli` + `ggml-small.bin`).

Cách Youwee làm: nén tiếng thành MP3 một kênh, khoảng 64 kbps (32 kbps nếu file vẫn lớn), rồi gửi lên API Whisper. API OpenAI nhận tối đa 25 MB. Máy chỉ chạy FFmpeg một lúc để nén.

Đánh đổi: file tiếng rời máy. Hiện app không gửi audio. Cần key và mạng. Video dài phải cắt thành nhiều đoạn dưới mức 25 MB rồi ghép lại mốc thời gian.

## 2. Dịch

Đã chuyển được, không cần viết thêm. Gemini, ChatGPT, Claude và DeepSeek chỉ nhận chữ lời thoại.

Nếu đang chọn "Trên máy" (Ollama, `qwen3.5:9b-q4_K_M` hoặc `gemma3:12b`) thì đây là chỗ làm máy nặng sau Whisper. Đổi sang một API key là máy khỏi chạy model dịch.

Đánh đổi: chữ lời thoại đi qua mạng. Muốn offline hẳn thì giữ Ollama.

## 3. Đọc giọng

Hiện đọc bằng Piper trên máy: `piper.exe` nếu có, không thì ONNX trong cửa sổ app, một luồng. Chữ để đọc không rời máy.

Có thể chuyển sang một dịch vụ đọc giọng (kiểu Edge-TTS hoặc API TTS): gửi câu đã dịch, nhận wav về. Máy khỏi chạy ONNX.

Đánh đổi: chữ cần đọc đi qua mạng, giọng phụ thuộc dịch vụ, và không còn là đường mặc định hiện tại.

## Chưa đáng chuyển

Tách tiếng, xếp câu, ghép video (hình copy, chỉ mã hóa tiếng AAC) là việc ngắn của FFmpeg. Hàng đợi nhiều video cũng ở trên máy: xếp chờ rồi chạy từng video một. Hàng đợi không làm một video nhẹ hơn; nó chỉ tránh chạy hai video cùng lúc.
