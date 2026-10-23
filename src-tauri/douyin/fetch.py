"""Tải một video Douyin công khai.

Edge headless mở trang để Douyin cấp cookie khách (uifid). Phần ký
a_bogus và x-secsdk-web-signature nằm cạnh file này, Apache-2.0, lấy từ
Douyin_TikTok_Download_API. File mp4 ghi vào folder được truyền vào.
"""

from __future__ import annotations

import json
import os
import socket
import subprocess
import sys
import time
import urllib.error
import urllib.request
from base64 import b64encode
from pathlib import Path
from urllib.parse import quote, unquote, urlencode

sys.path.insert(0, str(Path(__file__).resolve().parent))
from dtk.signing.native.abogus import ABogus, build_browser_info
from dtk.signing.native.websign import UIFID_COOKIE_NAMES, sign

PAGE = "https://www.douyin.com/"


def aweme_from(url: str) -> str | None:
    """Id nằm sẵn trong link dài. Link rút gọn thì chưa có."""
    query = url.split("?", 1)[1].split("#", 1)[0] if "?" in url else ""
    for part in query.split("&"):
        if part.startswith("modal_id="):
            value = part.split("=", 1)[1]
            if value.isdigit() and 16 <= len(value) <= 20:
                return value
    path = url.split("://", 1)[-1].split("?", 1)[0].split("#", 1)[0]
    bits = [bit for bit in path.split("/") if bit]
    if len(bits) >= 2 and bits[-2] == "video" and bits[-1].isdigit() and 16 <= len(bits[-1]) <= 20:
        return bits[-1]
    return None


def windows_name(title: str, aweme: str) -> str:
    cleaned = []
    for char in title.replace("\n", " "):
        cleaned.append(" " if char in '\\/:*?"<>|' or ord(char) < 32 else char)
    text = " ".join("".join(cleaned).split()).strip(" .") or "douyin"
    raw = text.encode("utf-8")
    if len(raw) > 120:
        text = raw[:120].decode("utf-8", "ignore").rstrip()
    return f"{text} [{aweme}].mp4"


def edge_path() -> str | None:
    roots = [
        os.environ.get("PROGRAMFILES(X86)", ""),
        os.environ.get("PROGRAMFILES", ""),
        os.environ.get("LOCALAPPDATA", ""),
    ]
    for root in roots:
        if not root:
            continue
        candidate = Path(root) / "Microsoft" / "Edge" / "Application" / "msedge.exe"
        if candidate.is_file():
            return str(candidate)
    return None


def read_exact(sock: socket.socket, count: int) -> bytes:
    buf = b""
    while len(buf) < count:
        chunk = sock.recv(count - len(buf))
        if not chunk:
            raise EOFError("socket closed")
        buf += chunk
    return buf


class Cdp:
    def __init__(self, ws_url: str) -> None:
        rest = ws_url.split("://", 1)[1]
        hostport, path = rest.split("/", 1)
        host, port = hostport.split(":")
        self.sock = socket.create_connection((host, int(port)), timeout=20)
        key = b64encode(os.urandom(16)).decode()
        request = (
            f"GET /{path} HTTP/1.1\r\n"
            f"Host: {hostport}\r\n"
            "Upgrade: websocket\r\n"
            "Connection: Upgrade\r\n"
            f"Sec-WebSocket-Key: {key}\r\n"
            "Sec-WebSocket-Version: 13\r\n\r\n"
        )
        self.sock.sendall(request.encode())
        head = b""
        while b"\r\n\r\n" not in head:
            head += self.sock.recv(1)
        if b" 101 " not in head.split(b"\r\n", 1)[0]:
            raise RuntimeError("Không nối được Edge.")
        self.next_id = 1

    def send(self, text: str) -> None:
        data = text.encode()
        mask = os.urandom(4)
        frame = bytearray([0x81])
        size = len(data)
        if size < 126:
            frame.append(0x80 | size)
        elif size < 65536:
            frame.append(0x80 | 126)
            frame += size.to_bytes(2, "big")
        else:
            frame.append(0x80 | 127)
            frame += size.to_bytes(8, "big")
        frame += mask
        frame += bytes(byte ^ mask[index % 4] for index, byte in enumerate(data))
        self.sock.sendall(frame)

    def recv(self) -> str | None:
        first, second = read_exact(self.sock, 2)
        opcode = first & 0x0F
        length = second & 0x7F
        if length == 126:
            length = int.from_bytes(read_exact(self.sock, 2), "big")
        elif length == 127:
            length = int.from_bytes(read_exact(self.sock, 8), "big")
        if second & 0x80:
            mask = read_exact(self.sock, 4)
            data = bytes(byte ^ mask[index % 4] for index, byte in enumerate(read_exact(self.sock, length)))
        else:
            data = read_exact(self.sock, length)
        if opcode == 0x8:
            raise EOFError("ws close")
        return data.decode("utf-8", "replace") if opcode == 0x1 else None

    def call(self, method: str, params: dict | None = None, timeout: float = 20) -> dict:
        call_id = self.next_id
        self.next_id += 1
        payload: dict = {"id": call_id, "method": method}
        if params:
            payload["params"] = params
        self.send(json.dumps(payload))
        deadline = time.time() + timeout
        while time.time() < deadline:
            self.sock.settimeout(max(0.2, deadline - time.time()))
            raw = self.recv()
            if not raw:
                continue
            message = json.loads(raw)
            if message.get("id") == call_id:
                if "error" in message:
                    raise RuntimeError(str(message["error"])[:200])
                return message.get("result", {})
        raise TimeoutError(method)


def http_json(url: str) -> object:
    with urllib.request.urlopen(url, timeout=5) as response:
        return json.load(response)


def free_port() -> int:
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        return probe.getsockname()[1]


def chrome_version(user_agent: str) -> str:
    mark = "Chrome/"
    start = user_agent.find(mark)
    if start < 0:
        return "130.0.0.0"
    return user_agent[start + len(mark) :].split(" ", 1)[0]


def detail_query(aweme: str, user_agent: str, screen: dict) -> tuple[str, dict]:
    version = chrome_version(user_agent)
    params = {
        "device_platform": "webapp",
        "aid": "6383",
        "channel": "channel_pc_web",
        "pc_client_type": "1",
        "version_code": "290100",
        "version_name": "29.1.0",
        "cookie_enabled": "true",
        "screen_width": str(screen["sw"]),
        "screen_height": str(screen["sh"]),
        "browser_language": "zh-CN",
        "browser_platform": "Win32",
        "browser_name": "Chrome",
        "browser_version": version,
        "browser_online": "true",
        "engine_name": "Blink",
        "engine_version": version,
        "os_name": "Windows",
        "os_version": "10",
        "cpu_core_num": str(screen.get("cores") or 8),
        "device_memory": str(screen.get("mem") or 8),
        "platform": "PC",
        "downlink": "10",
        "effective_type": "4g",
        "round_trip_time": "0",
        "update_version_code": "170400",
        "aweme_id": aweme,
    }
    query = urlencode(params)
    bogus = ABogus(user_agent, browser_info=screen["info"]).get_value(query)
    query = f"{query}&a_bogus={quote(bogus, safe='')}"
    pairs = []
    for part in query.split("&"):
        name, _, value = part.partition("=")
        pairs.append((unquote(name), unquote(value)))
    verify = screen.get("verify") or ""
    if verify:
        pairs.append(("verifyFp", verify))
        pairs.append(("fp", verify))
    signed, _signature, extra = sign(pairs, screen["uifid"])
    return signed, extra


def play_urls(detail: dict) -> list[str]:
    video = detail.get("video") or {}
    ranked: list[tuple[int, str]] = []
    for gear in video.get("bit_rate") or []:
        height = int(gear.get("height") or 0)
        if height > 1080:
            continue
        for url in (gear.get("play_addr") or {}).get("url_list") or []:
            ranked.append((height, url))
    ranked.sort(key=lambda item: item[0], reverse=True)
    urls = [url for _height, url in ranked]
    urls.extend((video.get("play_addr") or {}).get("url_list") or [])
    clean = [url for url in urls if "playwm" not in url]
    return clean or urls


def session(page_url: str) -> tuple[subprocess.Popen, Cdp, dict]:
    binary = edge_path()
    if not binary:
        raise RuntimeError("Chưa thấy Microsoft Edge. Tải Douyin cần Edge.")
    profile = Path(os.environ["TEMP"]) / f"dy-edge-{os.getpid()}"
    if profile.exists():
        subprocess.run(["cmd", "/c", "rmdir", "/s", "/q", str(profile)], check=False)
    profile.mkdir(parents=True, exist_ok=True)
    port = free_port()
    proc = subprocess.Popen(
        [
            binary,
            "--headless=new",
            "--disable-gpu",
            f"--remote-debugging-port={port}",
            f"--user-data-dir={profile}",
            "--no-first-run",
            "--no-default-browser-check",
            "--window-size=1280,800",
            "about:blank",
        ],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    try:
        tabs = None
        for _ in range(40):
            try:
                tabs = http_json(f"http://127.0.0.1:{port}/json")
                break
            except Exception:
                time.sleep(0.25)
        if not isinstance(tabs, list) or not tabs:
            raise RuntimeError("Edge không mở được cổng gỡ lỗi.")
        page = next(item for item in tabs if item.get("type") == "page")
        cdp = Cdp(page["webSocketDebuggerUrl"])
        cdp.call("Network.enable")
        cdp.call("Page.enable")
        cdp.call("Page.navigate", {"url": page_url})
        screen = wait_session(cdp, page_url)
        return proc, cdp, screen
    except Exception:
        proc.kill()
        raise


def wait_session(cdp: Cdp, page_url: str) -> dict:
    known = aweme_from(page_url)
    deadline = time.time() + 20
    last_href = page_url
    while time.time() < deadline:
        listed = cdp.call(
            "Network.getCookies",
            {"urls": [PAGE, "https://www-hj.douyin.com/"]},
        )
        cookies = {item["name"]: item["value"] for item in listed.get("cookies", [])}
        uifid = next((cookies[name] for name in UIFID_COOKIE_NAMES if cookies.get(name)), "")
        raw = cdp.call(
            "Runtime.evaluate",
            {
                "expression": """JSON.stringify({
                  href: location.href,
                  ua: navigator.userAgent,
                  iw: innerWidth, ih: innerHeight,
                  ow: outerWidth, oh: outerHeight,
                  aw: screen.availWidth, ah: screen.availHeight,
                  sw: screen.width, sh: screen.height,
                  p: navigator.platform,
                  cores: navigator.hardwareConcurrency || 8,
                  mem: navigator.deviceMemory || 8
                })""",
                "returnByValue": True,
            },
        )
        measured = json.loads(raw["result"]["value"])
        last_href = measured.get("href") or last_href
        aweme = aweme_from(last_href) or known
        if uifid and aweme and int(measured.get("sw") or 0) > 0:
            info = build_browser_info(
                inner_width=int(measured["iw"]),
                inner_height=int(measured["ih"]),
                outer_width=int(measured["ow"]),
                outer_height=int(measured["oh"]),
                avail_width=int(measured["aw"]),
                avail_height=int(measured["ah"]),
                screen_width=int(measured["sw"]),
                screen_height=int(measured["sh"]),
                platform=str(measured["p"]),
            )
            return {
                "ua": measured["ua"],
                "info": info,
                "sw": int(measured["sw"]),
                "sh": int(measured["sh"]),
                "cores": measured.get("cores") or 8,
                "mem": measured.get("mem") or 8,
                "uifid": uifid,
                "verify": cookies.get("s_v_web_id", ""),
                "cookie": "; ".join(f"{name}={value}" for name, value in cookies.items() if value),
                "aweme": aweme,
            }
        time.sleep(0.4)
    if not aweme_from(last_href) and not known:
        raise RuntimeError("Không đọc được mã video từ link Douyin.")
    raise RuntimeError("Edge không lấy được phiên Douyin. Thử lại.")


def detail_for(screen: dict) -> dict:
    signed, extra = detail_query(screen["aweme"], screen["ua"], screen)
    request = urllib.request.Request(
        "https://www.douyin.com/aweme/v1/web/aweme/detail/?" + signed,
        headers={
            "User-Agent": screen["ua"],
            "Referer": f"{PAGE}video/{screen['aweme']}",
            "Accept": "application/json, text/plain, */*",
            "Accept-Language": "zh-CN,zh;q=0.9",
            "Cookie": screen["cookie"],
            **extra,
        },
    )
    with urllib.request.urlopen(request, timeout=25) as response:
        body = response.read()
    data = json.loads(body.decode("utf-8", "replace"))
    detail = data.get("aweme_detail")
    if not isinstance(detail, dict) or not play_urls(detail):
        raise RuntimeError("Douyin không trả địa chỉ video.")
    return detail


def save_video(urls: list[str], dest: Path, screen: dict) -> None:
    headers = {
        "User-Agent": screen["ua"],
        "Referer": PAGE,
        "Cookie": screen["cookie"],
    }
    for url in urls:
        request = urllib.request.Request(url, headers=headers)
        try:
            response = urllib.request.urlopen(request, timeout=30)
        except (urllib.error.URLError, TimeoutError):
            continue
        with response:
            total = response.headers.get("Content-Length")
            total_n = float(total) if total and total.isdigit() else None
            started = time.time()
            written = 0
            last = 0.0
            with dest.open("wb") as handle:
                while True:
                    chunk = response.read(256 * 1024)
                    if not chunk:
                        break
                    if written == 0 and (b"<html" in chunk[:200].lower() or b"ftyp" not in chunk[:32] and chunk[:1] == b"<"):
                        break
                    handle.write(chunk)
                    written += len(chunk)
                    now = time.time()
                    if now - last >= 0.4:
                        speed = written / max(now - started, 0.1)
                        eta = (total_n - written) / speed if total_n and speed else None
                        total_text = str(int(total_n)) if total_n else "NA"
                        eta_text = str(int(eta)) if eta and eta >= 0 else "NA"
                        print(
                            f"DLP downloading {written} {total_text} NA {speed:.1f} {eta_text}",
                            file=sys.stderr,
                            flush=True,
                        )
                        last = now
            if written > 100_000 and dest.is_file():
                print(f"DLP finished {written} {written} NA NA 0", file=sys.stderr, flush=True)
                return
            dest.unlink(missing_ok=True)
    raise RuntimeError("Tải file video thất bại.")


def download(page_url: str, folder: str) -> Path:
    out = Path(folder)
    if not out.is_dir():
        raise RuntimeError("Folder kết quả chưa có.")
    proc, _cdp, screen = session(page_url)
    try:
        suffix = f"[{screen['aweme']}].mp4"
        existing = next(
            (path for path in out.iterdir() if path.name.endswith(suffix) and path.stat().st_size > 100_000),
            None,
        )
        if existing:
            return existing
        loaded = detail_for(screen)
        title = str(loaded.get("desc") or "douyin")
        dest = out / windows_name(title, screen["aweme"])
        save_video(play_urls(loaded), dest, screen)
        return dest
    finally:
        proc.kill()


def self_test() -> None:
    assert aweme_from("https://www.douyin.com/video/7691382497195625743") == "7691382497195625743"
    assert aweme_from("https://www.douyin.com/jingxuan?modal_id=7691382497195625743") == "7691382497195625743"
    assert aweme_from("https://v.douyin.com/TpJLQyTXRPk/") is None
    assert windows_name("a/b:c", "1234567890123456").endswith("[1234567890123456].mp4")
    assert "/" not in windows_name("a/b", "1234567890123456")


def main() -> None:
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")
    if len(sys.argv) == 2 and sys.argv[1] == "--self-test":
        self_test()
        return
    if len(sys.argv) != 3:
        raise RuntimeError("Thiếu link hoặc folder kết quả.")
    path = download(sys.argv[1], sys.argv[2])
    print(path, flush=True)


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
