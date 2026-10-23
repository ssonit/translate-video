import { LOCAL_MODELS } from "./guides";

type Props = {
  installed: string[] | null;
  note: string;
  copied: string;
  onCopy: (command: string) => void;
  // "all" trên màn chuẩn bị: model đã có và model chưa có. "missing" trên màn lồng: chỉ lệnh tải.
  mode: "all" | "missing";
};

export default function LocalModels({ installed, note, copied, onCopy, mode }: Props) {
  const known = installed !== null;
  const rows = LOCAL_MODELS.filter((model) => {
    if (!known || mode === "all") return true;
    return !installed.includes(model.name);
  });
  if (rows.length === 0) return null;

  return (
    <div className="model-cards">
      {mode === "all" ? (
        <p className="key-note">Ba model này là gợi ý, chọn theo bộ nhớ card. Muốn model khác thì ở màn lồng tiếng chọn Khác và gõ tên.</p>
      ) : null}
      {note ? <p className="key-note">{note}</p> : null}
      {rows.map((model) => {
        const ready = known && installed.includes(model.name);
        return (
          <div className="command" key={model.name}>
            <div>
              <strong>{model.label}</strong>
              <p>
                {model.when} {ready ? "Đã có trên máy." : known ? "Chưa tải." : note ? "Chưa kiểm tra được." : "Đang kiểm tra."}{" "}
                {model.note}
              </p>
              {ready || (!known && !note) ? null : <code>{model.command}</code>}
            </div>
            {ready ? (
              <span className="mark is-ready">Đã có</span>
            ) : known || note ? (
              <button type="button" onClick={() => onCopy(model.command)}>
                {copied === model.command ? "Đã chép" : "Chép lệnh"}
              </button>
            ) : null}
          </div>
        );
      })}
    </div>
  );
}
