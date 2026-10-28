import { useId } from "react";

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
  const group = useId();
  const mode = textOnly ? "captions" : audioOnly ? "audio" : "video";
  return (
    <div className="output-options">
      <fieldset className="output-choices" disabled={disabled}>
        <legend>Loại kết quả</legend>
        {[
          { id: "video", label: "Video lồng tiếng", note: "Giọng mới cùng hình gốc" },
          { id: "audio", label: "Audio", note: "Chỉ xuất tiếng, không cần hình" },
          { id: "captions", label: "Video phụ đề", note: "Giữ tiếng gốc, thêm chữ" },
        ].map((item) => (
          <label key={item.id} className="output-choice">
            <input type="radio" name={group} value={item.id} checked={mode === item.id} onChange={() => {
              if (item.id === "audio") onAudioOnly(true);
              else if (item.id === "captions") onTextOnly(true);
              else { onAudioOnly(false); onTextOnly(false); }
            }} />
            <span><strong>{item.label}</strong><small>{item.note}</small></span>
          </label>
        ))}
      </fieldset>
      {mode === "video" ? (
        <div className="run-options">
          <label className="redo" title="Ghi chữ lên hình. Không chọn thì vẫn có file phụ đề riêng.">
            <input type="checkbox" checked={burnCaptions} disabled={disabled} onChange={(event) => onBurn(event.target.checked)} />
            Kèm phụ đề trên video
          </label>
        </div>
      ) : null}
    </div>
  );
}
