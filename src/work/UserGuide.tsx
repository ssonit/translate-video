import type { ReactNode } from "react";
import "./UserGuide.css";

const topics = [
  ["start", "Video đầu tiên"],
  ["prepare", "Chuẩn bị & dịch vụ dịch"],
  ["output", "Chọn loại kết quả"],
  ["preview", "Xem và nghe trước"],
  ["style", "Hình, chữ & âm thanh"],
  ["edit", "Sửa và lưu kết quả"],
  ["queue", "Hàng đợi & chạy tiếp"],
  ["questions", "Khi gặp vướng mắc"],
] as const;

function Topic({ id, title, children }: { id: string; title: string; children: ReactNode }) {
  return <section className="guide-topic" id={`guide-${id}`} aria-labelledby={`guide-title-${id}`}>
    <h3 id={`guide-title-${id}`} tabIndex={-1}>{title}</h3>{children}
  </section>;
}

export function UserGuide({ onTools, onStart, onSettings }: {
  onTools: () => void; onStart: () => void; onSettings: () => void;
}) {
  return <div className="pane-scroll studio-page user-guide">
    <header className="studio-page-heading guide-heading">
      <p className="kicker">Từng bước một</p>
      <h2>Hướng dẫn sử dụng</h2>
      <p>Từ video gốc đến bản lồng tiếng của bạn. Bắt đầu với một video ngắn để chọn giọng và kiểm tra lời dịch.</p>
      <div className="guide-actions">
        <button type="button" className="check" onClick={onStart}>Bắt đầu lồng tiếng</button>
        <button type="button" className="link" onClick={onTools}>Kiểm tra đồ nghề</button>
      </div>
    </header>
    <div className="guide-layout">
      <nav className="guide-contents" aria-label="Mục lục hướng dẫn">
        <p>Trong hướng dẫn này</p>
        {topics.map(([id, label], index) => <button type="button" key={id} onClick={() => {
          const section = document.getElementById(`guide-${id}`);
          section?.scrollIntoView({ block: "start" });
          section?.querySelector("h3")?.focus({ preventScroll: true });
        }}><span>{String(index + 1).padStart(2, "0")}</span>{label}</button>)}
      </nav>
      <article className="guide-article">
        <Topic id="start" title="Lồng video đầu tiên">
          <ol className="guide-steps">
            <li><strong>Chọn nơi lưu.</strong> Vào <button type="button" className="guide-inline" onClick={onSettings}>Cài đặt</button> và chọn folder kết quả.</li>
            <li><strong>Chọn nguồn.</strong> Ở màn Lồng tiếng, chọn <b>File trên máy → Chọn video</b>, hoặc dán link một video YouTube, Douyin hay TikTok vào <b>Link video</b>. Douyin và TikTok nhận cả đoạn chữ chia sẻ; YouTube cần URL riêng. Không nhận playlist.</li>
            <li><strong>Chọn kết quả và giọng.</strong> Chọn Video lồng tiếng, ngôn ngữ đích, giọng đọc và dịch vụ trong <b>Dịch bằng</b>. Nghe thử giọng; nhập và lưu API key nếu dịch vụ cần key.</li>
            <li><strong>Bấm Chạy.</strong> Theo dõi tại <b>Hàng đợi</b>. Khi app chờ duyệt tên, sửa tên chép sai hoặc giữ danh sách rồi bấm <b>Dịch tiếp</b>.</li>
            <li><strong>Xem và lưu.</strong> Kiểm tra video hoàn thành, dùng <b>Mở folder</b> để lấy kết quả hoặc <b>Tải video</b> để lưu bản MP4 sang nơi khác.</li>
          </ol>
          <aside className="guide-note">Giữ cửa sổ app mở trong lúc xử lý. Giọng đọc có thể chạy ngay trong cửa sổ này.</aside>
        </Topic>
        <Topic id="prepare" title="Chuẩn bị máy và chọn cách dịch">
          <p>Bấm trạng thái ở góc phải để mở Đồ nghề. <b>Đang kiểm tra</b> là chưa có kết quả; <b>Thiếu …, xem</b> cho biết mục cần bổ sung. Thiếu công cụ bắt buộc sẽ khóa chạy và xem/nghe trước.</p>
          <ul>
            <li><b>FFmpeg và ffprobe:</b> cần có để xử lý hình và tiếng. Mục thiếu trong Đồ nghề có cách chuẩn bị.</li>
            <li><b>Whisper:</b> bấm <b>Tải Whisper</b> trong mục Whisper để tải chương trình và model còn thiếu hoặc cần cập nhật. Có thể hủy khi đang tải.</li>
            <li><b>Giọng đọc:</b> chọn giọng có sẵn. Ngôn ngữ khác cần cặp file .onnx và .onnx.json trong folder voices tương ứng; app không tự tải giọng.</li>
            <li><b>yt-dlp:</b> chỉ cần khi lấy nguồn YouTube hoặc TikTok. Douyin dùng Edge trên máy và không cần bạn dán cookie.</li>
          </ul>
          <div className="guide-options">
            <div><h4>Dùng API key</h4><p>Chọn Gemini, ChatGPT, Claude hoặc DeepSeek API, nhập key rồi bấm <b>Lưu key</b>. Không cần Ollama. Chi phí dịch tùy tài khoản dịch vụ.</p></div>
            <div><h4>DeepSeek local</h4><p>Chuẩn bị server Deepseek-API rồi bấm <b>Bật DeepSeek</b>, chờ server sẵn sàng. Không dùng API key; vẫn kết nối tài khoản chat.deepseek.com qua mạng.</p></div>
            <div><h4>Trên máy · Ollama</h4><p>Chuẩn bị Ollama với <b>qwen3.5:9b-q4_K_M</b> hoặc <b>gemma3:12b</b>, rồi chọn model đã có. App không tự tải model.</p></div>
          </div>
          <p>Bật <b>Không dịch</b> để giữ lời gốc. Nếu xuất giọng đọc, chọn giọng phù hợp với ngôn ngữ gốc.</p>
        </Topic>
        <Topic id="output" title="Bạn muốn nhận kết quả gì?">
          <div className="guide-options">
            <div><h4>Video lồng tiếng</h4><p>Video có giọng mới. Bật <b>Kèm phụ đề trên video</b> nếu muốn ghi chữ trực tiếp lên hình.</p></div>
            <div><h4>Audio</h4><p>Chỉ xuất tiếng, không xuất hình. Dùng <b>Nghe trước</b> để thử rồi <b>Xuất audio</b> khi sẵn sàng.</p></div>
            <div><h4>Video phụ đề</h4><p>Giữ tiếng gốc, ghi lời thoại lên hình. Chế độ này không tạo giọng đọc mới.</p></div>
          </div>
        </Topic>
        <Topic id="preview" title="Xem và nghe trước khi xuất">
          <p><b>Xem trước</b> phát video gốc trong khi app chép lời, dịch và đọc từng câu. Nếu gần hết phần đã xử lý, video tạm dừng ở trạng thái <b>Đang dịch tiếp</b>. Chờ phần sau sẵn sàng để xem tiếp.</p>
          <p>Với Audio, dùng <b>Nghe trước</b>. Nguồn link có phụ đề có thể chia thành nhiều phần; chọn phần để dịch và nghe phần đó.</p>
          <aside className="guide-note">Xem/nghe trước chưa qua duyệt tên và thuật ngữ như khi xuất. Xem trước cũng chưa áp dụng căn độ dài giọng, khung xuất và kiểu chữ. Kiểm tra bản xuất hoàn chỉnh để đánh giá kết quả cuối.</aside>
          <p>Sau khi xem, dùng <b>Ghép video</b> hoặc <b>Xuất audio</b>. Dừng giữa chừng có thể cần xử lý lại phần xem trước chưa hoàn thành.</p>
        </Topic>
        <Topic id="style" title="Chỉnh hình, phụ đề và âm thanh">
          <p>Mở <b>Hình ảnh, chữ & âm thanh</b> trên màn Lồng tiếng. Chọn nhóm cấu hình cần chỉnh rồi bấm <b>Dùng cấu hình này</b> để quay lại.</p>
          <ul>
            <li><b>Hình & chữ:</b> giữ khung gốc hoặc chọn 9:16, 1:1, 16:9. Giữ toàn hình với nền mờ hoặc cắt đầy khung, kéo hình để giữ nhân vật.</li>
            <li><b>Phụ đề:</b> bật ghi chữ trên video để chỉnh font, cỡ, màu, viền, nền và vị trí. Vùng gợi ý tránh giao diện video ngắn không ghi vào kết quả.</li>
            <li><b>Logo và che chữ:</b> thêm logo hoặc vùng che màu/làm mờ chữ nguồn. App không tự nhận diện hay dịch chữ đã có trong hình.</li>
            <li><b>Âm thanh:</b> chỉnh tiếng gốc, giọng mới và hạ tiếng gốc khi có giọng đọc. Cách đọc tên chỉ đổi phát âm, giữ nguyên chữ trên phụ đề.</li>
            <li><b>Đoạn thử 8 giây:</b> dựng từ bản dịch và tiếng từng câu đã có; cần chạy xử lý trước để có dữ liệu thử.</li>
          </ul>
          <p>Bấm <b>Lưu bộ mới</b> để dùng lại phong cách. Chọn và quản lý trong <b>Bộ cài đặt</b>; bộ lưu không chứa key và không tự đổi dịch vụ dịch.</p>
        </Topic>
        <Topic id="edit" title="Sửa lời và lưu bản hoàn chỉnh">
          <p>Mở kết quả từ <b>Thư viện</b>. Kiểm tra tên riêng, số và câu dài; dùng <b>Đến câu cần kiểm tra</b> để duyệt các câu được đánh dấu.</p>
          <ol>
            <li>Bấm <b>Sửa</b> ở câu cần chỉnh, sửa lời rồi bấm <b>Xong</b> để đưa vào nháp.</li>
            <li>Mở timeline để <b>Chỉnh thời gian câu</b>: nhập mốc hoặc kéo câu trên sóng âm. Có thể tách tại vị trí phát hoặc gộp với câu sau. Audio liền mạch không dùng phần chỉnh mốc.</li>
            <li>Bấm <b>Xuất lại</b> để áp dụng. Nháp chưa thay bản đang phát; đổi riêng mốc giữ tiếng đã đọc, đổi lời có thể cần đọc lại câu đó.</li>
          </ol>
          <p>Muốn đổi bố cục hay âm lượng, bấm <b>Hình ảnh & âm thanh</b>, chỉnh rồi chọn <b>Xuất theo cấu hình</b>. Trong <b>Bản xuất</b>, xem lịch sử và cấu hình của các phiên bản; <b>Nạp cấu hình để sửa tiếp</b> dùng lời và mốc hiện tại.</p>
          <p><b>Tải video</b> lưu đúng bản MP4 đang xem, kể cả bản có chữ hoặc bản lịch sử. Nút có thể nằm trong menu ba chấm trên video. Dùng <b>Mở folder</b> để lấy file audio và <b>Mở phụ đề .srt</b> để xem phụ đề riêng khi đã tạo.</p>
        </Topic>
        <Topic id="queue" title="Làm nhiều video và tiếp tục việc dở">
          <p>Khi một video đang chạy, quay lại Lồng tiếng, chọn nguồn khác rồi bấm <b>Thêm vào hàng</b>. Mỗi việc giữ cấu hình lúc thêm. Muốn đổi một việc đang chờ, dùng <b>Dùng lựa chọn hiện tại</b> trong hàng đợi.</p>
          <p>Mặc định hàng đợi dừng khi lỗi. Đọc thông báo, xử lý nguyên nhân rồi thử lại. Bước duyệt tên vẫn cần bạn xác nhận.</p>
          <p>Nếu bấm <b>Dừng</b> hoặc đóng app, giữ nguyên nguồn và folder kết quả. Mở việc dở trong Thư viện và bấm <b>Chạy tiếp</b> để dùng lại các bước đã xong.</p>
          <aside className="guide-note">Giữ cả folder việc nếu muốn sửa và xuất lại nhanh. Nếu app báo nguồn khác trùng tên, đổi tên video mới hoặc chọn folder kết quả khác.</aside>
        </Topic>
        <Topic id="questions" title="Khi gặp vướng mắc">
          <div className="guide-faq">
            <details><summary>Vì sao nút Chạy bị khóa?</summary><p>Kiểm tra nguồn, nơi lưu và key đã lưu. Nếu có dòng báo thiếu công cụ, bấm <b>Xem đồ nghề</b> để bổ sung mục thiếu.</p></details>
            <details><summary>App dừng ở bước tên có phải bị lỗi?</summary><p>Đây là bước chờ bạn duyệt. Sửa tên chép sai hoặc giữ danh sách rồi bấm <b>Dịch tiếp</b> tại Hàng đợi.</p></details>
            <details><summary>Không có giọng cho ngôn ngữ tôi chọn?</summary><p>Cần cặp .onnx và .onnx.json trong voices/&lt;mã ngôn ngữ&gt;, chẳng hạn voices/vi cho tiếng Việt. App không tự tải giọng.</p></details>
            <details><summary>Câu đọc quá nhanh hoặc bị cắt thì sửa thế nào?</summary><p>Tìm câu được đánh dấu, rút gọn lời hoặc chỉnh mốc nếu có khoảng trống rồi xuất lại. Nghe lại đoạn chuyển giữa các câu sau khi xuất.</p></details>
            <details><summary>Không tải được link video?</summary><p>Dùng link một video được hỗ trợ. YouTube và TikTok cần yt-dlp; Douyin cần Edge. Với YouTube hãy dán riêng URL. Đọc thông báo tải để xác định nguyên nhân.</p></details>
            <details><summary>Video hay tiếng của tôi có được gửi lên mạng?</summary><p>Audio và video xử lý trên máy. Khi dùng API hoặc DeepSeek local, chữ lời thoại đi tới dịch vụ cho các bước AI. Giọng đọc tạo trên máy. Muốn xử lý hoàn toàn trên máy, dùng file local và Ollama với công cụ, model, giọng đã chuẩn bị sẵn.</p></details>
          </div>
        </Topic>
      </article>
    </div>
  </div>;
}
