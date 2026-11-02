import { useId } from "react";
import { maskHintShown } from "./format";

export function RunOptions({
  audioOnly,
  textOnly,
  burnCaptions,
  disabled,
  onAudioOnly,
  onTextOnly,
  onBurn,
  maskOn,
  onMask,
}: {
  audioOnly: boolean;
  textOnly: boolean;
  burnCaptions: boolean;
  disabled: boolean;
  onAudioOnly: (checked: boolean) => void;
  onTextOnly: (checked: boolean) => void;
  onBurn: (checked: boolean) => void;
  // Chỉ màn việc mới truyền hai mục này. Không truyền thì không có gợi ý về chữ cứng.
  maskOn?: boolean;
  onMask?: () => void;
}) {
  const group = useId();
  const mode = textOnly ? "captions" : audioOnly ? "audio" : "video";
  const maskHint = onMask !== undefined && maskHintShown({ audioOnly, textOnly, burnCaptions, maskOn: maskOn === true });
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
      {maskHint ? (
        <p className="key-note">
          Video gốc có chữ cứng? Phụ đề mới sẽ đè lên chữ cũ.{" "}
          <button type="button" className="link" onClick={onMask}>Che / làm mờ chữ nguồn</button>
        </p>
      ) : null}
    </div>
  );
}
