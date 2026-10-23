export function RunOptions({
  audioOnly,
  textOnly,
  burnCaptions,
  disabled,
  onAudioOnly,
  onTextOnly,
  onBurn,
}: {
  audioOnly: boolean;
  textOnly: boolean;
  burnCaptions: boolean;
  disabled: boolean;
  onAudioOnly: (checked: boolean) => void;
  onTextOnly: (checked: boolean) => void;
  onBurn: (checked: boolean) => void;
}) {
  return (
    <div className="run-options">
      <label
        className="redo"
        title="Không tải video. Lưu link YouTube, lấy phụ đề hoặc tiếng. Cần hình thì bấm Tải video."
      >
        <input
          type="checkbox"
          checked={audioOnly}
          disabled={disabled}
          onChange={(event) => onAudioOnly(event.target.checked)}
        />
        Chỉ lấy audio
      </label>
      <label className="redo" title="Giữ tiếng gốc. Đốt chữ dịch lên hình. Không đọc giọng mới.">
        <input
          type="checkbox"
          checked={textOnly}
          disabled={disabled}
          onChange={(event) => onTextOnly(event.target.checked)}
        />
        Chỉ hiện chữ
      </label>
      <label
        className="redo"
        title={
          textOnly
            ? "Chế độ này luôn đốt chữ lên hình."
            : audioOnly
              ? "Ghi chữ chỉ dùng khi ghép video."
              : "Đốt lời đã dịch lên hình. Không chọn thì vẫn có file phụ đề, thêm chữ sau cũng được."
        }
      >
        <input
          type="checkbox"
          checked={textOnly || burnCaptions}
          disabled={disabled || audioOnly || textOnly}
          onChange={(event) => onBurn(event.target.checked)}
        />
        Ghi chữ vào video
      </label>
    </div>
  );
}
