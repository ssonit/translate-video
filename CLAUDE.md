# CLAUDE.md

Đọc `AGENTS.md` trước. File này nhắc luật cứng cho Claude Code.

## Việc đang làm

App desktop Tauri 2 + React để lồng tiếng video trên máy người dùng. Dịch bằng API key của AI khác (Gemini, ChatGPT, Claude, DeepSeek) hoặc Ollama trên máy, tùy người dùng chọn. Kết quả ghi vào folder local. Giao diện phải đủ tốt để thay pyVideoTrans.

Màn chuẩn bị đã có: kiểm tra công cụ, bấm mục thiếu thì hiện cách tải. App không tự tải gì, trừ nút "Tải Whisper". Màn lồng tiếng và pipeline 9 bước đã viết, unit test qua, chưa chạy thử với video thật. Chế độ **Chỉ hiện chữ** đốt chữ lên video gốc, không đọc giọng. Sau ngắt câu có bước Tên: lần đầu dừng để sửa tên bị chép sai (`names.json`, sự kiện `names-review`), lần sau áp danh sách đó. Xem trước không dừng. Nút **Xem trước** đã có (`start_watch`). Khi Chỉ lấy audio thì nút là **Nghe trước**, không tải hình. Có phụ đề thì dịch và phát từng phần ngay, không ghi wav. Đừng ghi translation.json trước khi cả video dịch xong.

## Luật

- Giữ Tauri. Đừng đề xuất chuyển Electron trừ khi người dùng yêu cầu.
- Offline. Không gửi video hoặc audio đi.
- Ollama không bắt buộc. Có API key Gemini, ChatGPT, Claude hoặc DeepSeek thì dịch bằng đó, không cần cài Ollama. Chỉ ai muốn dịch trên máy mới dùng Ollama tại `http://localhost:11434` (một model). Màn chuẩn bị không được chặn người dùng chỉ vì thiếu Ollama khi đã chọn dịch bằng API key.
- Giọng đọc local. Không lấy Edge-TTS làm mặc định.
- Màn chuẩn bị chủ yếu kiểm tra và hướng dẫn. Ngoại lệ duy nhất, do người dùng yêu cầu ngày 2026-10-07: nút "Tải Whisper" (chương trình whisper.cpp và model `ggml-small.bin`), phải kiểm tra phiên bản và mã băm SHA-256 trước khi dùng (`whisper_install.rs`). Đừng thêm nút tự tải FFmpeg, Ollama, model dịch hay installer nếu người dùng chưa yêu cầu.
- Tên app chưa chốt. Hỏi người dùng trước khi đổi `productName`, identifier, tiêu đề cửa sổ, hoặc chữ trên giao diện. `com.khung.studio` và `tauri-app` chỉ là placeholder scaffold.
- Các mục còn mở (ngôn ngữ giao diện, file local hay link, một giọng hay nhiều người nói) nằm trong `AGENTS.md`. Hỏi trước khi chọn giúp.
- Trả lời người dùng bằng tiếng Việt.
- Đừng commit nếu người dùng chưa yêu cầu.
