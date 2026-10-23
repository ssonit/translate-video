# Chế độ Xem dần

Ngày: 2026-10-12

## Mục tiêu

Người dùng dán link hoặc chọn file, rồi xem được trong app sau đoạn đầu, trong lúc phần sau vẫn đang chép, dịch và đọc. Trễ thêm khi máy không theo kịp là chấp nhận được. Video gốc phát trong cửa sổ. Trong mỗi câu đã đọc xong, tiếng gốc hạ xuống, phụ đề và giọng Piper nổi lên.

Chế độ ra file (`start_dub`, 8 bước, ghép mp4) giữ nguyên.

## Đã chốt

- Chạy trong app Tauri này. Không làm extension, không deploy web.
- Whisper local (`whisper-cli` + `ggml-small.bin`) và Piper local. Không thuê speech-to-text trên mây.
- Dịch vẫn theo lựa chọn hiện tại: API key Gemini, ChatGPT, Claude, DeepSeek, hoặc Ollama. Chữ lời thoại đi mạng chỉ khi người dùng đã chọn API.
- Không gửi audio hay video đi.
- Không thuật ngữ, không co độ dài, không ghép mp4 trong lúc xem.
- Nút **Ghép video** chỉ hiện sau khi cả video đã dịch và đọc xong. Nút đó gọi `start_dub`. Bước nào đã có file thì bỏ qua, nên phần còn lại là co độ dài và ghép.
- Giọng là giọng đang chọn cho ngôn ngữ đích. Thiếu giọng thì báo lỗi ngay từ đầu, cùng câu với chế độ file.

## Không làm

- Không hạ tiếng gốc bằng `muted`. Phần tử video bị mute thì không còn mẫu âm thanh để trộn.
- Không dùng Edge-TTS, không thêm nút tải mới.
- Không đổi tên app, `productName`, identifier, tiêu đề cửa sổ.
- Không nhận site mới. Link vẫn chỉ YouTube và Douyin, qua `download_video` như hiện tại.
- Không tua vào đoạn chưa dịch xong. Kéo quá mốc thì kéo lại đúng mốc đang có.
- Không ghi `translation.json` hay `lines/NNN.wav` khi mới xong một cụm.

## Kiến trúc

Một lần Whisper cho cả `audio.wav`, model nạp một lần. `whisper-cli` in từng câu ra stdout dạng `[00:01:02.000 --> 00:01:05.120]  lời`. Rust đọc dòng đó trong lúc process còn chạy.

Stdout và stderr không được đổ chung một bộ đệm. Dòng tiến độ (`progress = 45%`) nằm ở stderr. Một mảnh stdout đang dở mà bị nối với stderr sẽ làm mất câu.

Vòng đọc pipe chỉ phân tích và gửi cụm câu sang channel. Vòng kia dịch rồi gọi `tts::run`. Dịch nằm trong callback đọc pipe thì pipe đầy, Whisper dừng, hai bên chờ nhau.

Cụm được gửi đi khi một trong các điều này đúng với các câu chưa gửi:

- đủ 4 câu
- câu cuối kết thúc cách câu đầu ít nhất 12 giây
- thời điểm Whisper đã tiến (phần trăm × độ dài video) vượt quá cuối câu cuối ít nhất 8 giây
- Whisper đã thoát và thành công, gửi nốt phần còn lại

Ngưỡng nằm ở hằng số `FLUSH_COUNT = 4`, `FLUSH_SPAN_SECS = 12`, `FLUSH_AHEAD_SECS = 8` trong `src-tauri/src/watch.rs`.

Mỗi câu in ra được ngắt bằng `split_line` và `divide_time` như bước ngắt câu của chế độ file. Bảng thuật ngữ để trống. Câu dịch xong thì đọc vào `watch/lines/NNN.wav`. Chỉ số `NNN` là số thứ tự câu từ 1, không đổi trong suốt lượt.

## File

| Đường dẫn | Khi nào có | Chế độ file có đọc không |
|---|---|---|
| `audio.wav` | Sau tách tiếng | Có, bỏ qua tách tiếng nếu đã có |
| `transcript.json` | Whisper thoát mã 0 và tự ghi bằng `-oj`, như lệnh chép lời hiện tại | Có |
| `watch/partial.json` | Trong lúc xem, bản dịch chưa đủ | Không |
| `watch/lines/NNN.wav` | Trong lúc xem | Không |
| `lines.json`, `translation.json`, `lines/NNN.wav` | Chỉ khi mọi câu đã được dịch và đã có wav (kể cả wav im lặng cho câu không đọc được) | Có |
| `watch/done.json` | Ngay sau khi ba file canonical ở trên đã ghi xong | Không cần đọc để bỏ bước; sự có mặt của ba file canonical là đủ |

`start_dub` bỏ qua bước khi file canonical đã có. Vì vậy lượt xem dở không được để lại các file đó ở trạng thái thiếu câu.

Khi người dùng bấm Dừng:

- Xóa `watch/` (partial và wav nháp).
- Xóa `transcript.json` chỉ khi file này chưa có lúc bắt đầu lượt và Whisper không thoát mã 0.
- Giữ `transcript.json` nếu nó có từ trước, hoặc Whisper đã thoát mã 0 rồi người dùng mới dừng ở bước dịch hay đọc.

Lượt xem sau, trên cùng một folder việc:

- Đã đủ `transcript.json`, `lines.json`, `translation.json` và đủ wav từng câu: không chạy lại Whisper, dịch, hay đọc. Phát ngay.
- Có `transcript.json` mà chưa đủ bản dịch: không chạy Whisper. Lấy câu từ file đó, dịch và đọc từng cụm như lúc đang in.
- Chưa có transcript: chạy Whisper.

## Phát trong cửa sổ

Video phát là file gốc, qua `convertFileSrc`, sau khi Rust đã `allow_file` đúng file đó. Wav từng câu cũng được phép trước khi cửa sổ gắn vào thẻ audio.

Trước cụm đầu: video hiện, đang dừng, dòng trạng thái **Đang chép đoạn đầu**.

Khi `readyUntil` (cuối câu đã có wav) lớn hơn 0: tự phát, trừ khi người dùng đã bấm dừng.

Trong lúc pipeline còn chạy, nếu `readyUntil - currentTime < 1` giây (`HOLD_GAP_SECS`): dừng video, trạng thái **Đang dịch tiếp**. Hết khoảng chờ thì phát tiếp, chỉ khi người dùng không tự bấm dừng.

Người dùng bấm dừng trên thanh điều khiển thì không tự phát lại khi cụm sau xong.

Trong khoảng `start <= currentTime < end` của một câu đã có wav, và video không đang bị hold: `video.volume = 0.22` (`DUCK_VOLUME`). Ngoài khoảng đó, kể cả lúc hold: `video.volume = 1`. Thẻ `<audio>` của câu đang đọc để volume 1. Hết khoảng câu, hoặc sang câu sau, thì dừng thẻ audio đó. Câu đọc dài hơn khoảng của nó thì bị cắt ở cuối khoảng. Chế độ file mới là nơi co độ dài.

Kéo tay vượt `readyUntil` trong lúc còn chạy: `currentTime` quay về `readyUntil`.

Phụ đề là một lớp chữ trên khung video, lấy `translation` của câu đang chứa `currentTime`. Câu chưa có trong danh sách thì không hiện chữ. Lớp chữ không nhận click.

Hết pipeline: hết hold, phần đuôi video (nhạc, im lặng) phát với tiếng gốc mức 1.

Sự kiện `watch-update` gửi cả danh sách câu đã có wav, không gửi từng câu lệch. Payload:

```json
{
  "video": "đường dẫn file gốc",
  "readyUntil": 12.4,
  "running": true,
  "published": false,
  "detail": "Đang dịch tiếp",
  "lines": [
    {
      "index": 1,
      "start": 1.0,
      "end": 4.2,
      "source": "lời gốc",
      "translation": "lời dịch",
      "wav": "đường dẫn wav tuyệt đối"
    }
  ]
}
```

`published` chỉ đúng ở bản cập nhật sau khi đã ghi file canonical.

## Lỗi

- Không nghe được lời nào sau khi Whisper xong: báo **Không nghe được lời thoại.** và không phát.
- Thiếu giọng, thiếu ffmpeg, thiếu Whisper: cùng kiểu báo với chế độ file, trước khi phát.
- Một câu model không ra ngôn ngữ đích: để trống, wav im lặng, video đoạn đó giữ tiếng gốc mức 1. Không dừng cả lượt. Cùng cách `translate_work` đang làm.
- Hai lượt không chạy chồng. `start_watch` dùng cùng cờ `running` với `start_dub` và `download_video`.

## Giao diện

Màn làm việc, cạnh nút **Chạy**, thêm nút **Xem dần**. Cùng nguồn (file hoặc link), cùng folder, cùng ngôn ngữ đích. **Chạy** vẫn là ra file.

Màn đang xem dùng view `live`, không dùng view `watch` (view đó là video đã ghép). Có khung video, dòng trạng thái, phụ đề, và danh sách câu tăng dần. Khi `published` đúng, hiện nút **Ghép video**.

## Kiểm thử

Logic cụm, giờ Whisper, và quyết định xóa file có test Rust, không cần gọi Whisper thật. Logic hold, hạ tiếng, và tua có test Node trên hàm thuần trong `src/watchPlay.ts`. Một video thật vẫn là việc chạy tay sau khi code xong, vì pipeline file cũng chưa chạy thử với video thật.
