// Đoạn chia sẻ của Douyin và TikTok: mã lạ, tiêu đề, hashtag, rồi mới tới link.
// Giữ khớp với douyin_host và tiktok_host trong src-tauri/src/pipeline/download.rs.
const SHARE_HOSTS = new Set([
  "douyin.com",
  "www.douyin.com",
  "m.douyin.com",
  "v.douyin.com",
  "tiktok.com",
  "www.tiktok.com",
  "m.tiktok.com",
  "vm.tiktok.com",
  "vt.tiktok.com",
]);

function hostOf(link: string): string {
  try {
    return new URL(link).hostname;
  } catch {
    return "";
  }
}

// Dấu câu dính cuối link, cùng bộ ký tự với http_url trong download.rs.
const TRAILING_PUNCTUATION = /[.,，。!！)）\]」]+$/;

export function shareLink(text: string): string | null {
  const link = text.match(/https?:\/\/\S+/)?.[0].replace(TRAILING_PUNCTUATION, "");
  return link && SHARE_HOSTS.has(hostOf(link)) ? link : null;
}
