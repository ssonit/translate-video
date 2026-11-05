"""Tải một video Douyin công khai.

Edge headless mở trang để Douyin cấp cookie khách (uifid). Phần ký
a_bogus và x-secsdk-web-signature nằm cạnh file này, Apache-2.0, lấy từ
Douyin_TikTok_Download_API. File mp4 ghi vào folder được truyền vào.
"""

from __future__ import annotations

import ctypes
import http.client
import json
import os
import re
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
from base64 import b64encode
from ctypes import wintypes
from pathlib import Path
from urllib.parse import quote, unquote, urlencode

sys.path.insert(0, str(Path(__file__).resolve().parent))
from dtk.signing.native.abogus import ABogus, build_browser_info
from dtk.signing.native.websign import UIFID_COOKIE_NAMES, sign

PAGE = "https://www.douyin.com/"
DETAIL_URL = "https://www.douyin.com/aweme/v1/web/aweme/detail/"
# Rust (pipeline/download.rs, `douyin_pause`) tìm đúng chuỗi này trong stderr để biết Douyin đang chặn.
BLOCK_MARK = "[douyin-blocked]"
# Douyin từ chối yêu cầu thiếu hoặc sai chữ ký bằng 403; 429 là mã chuẩn của giới hạn tốc độ.
BLOCKING_CODES = (403, 429)


class Blocked(RuntimeError):
    """Douyin từ chối yêu cầu. Khác với video không còn: gặp lỗi này thì nên nghỉ, đừng hỏi dồn."""


def error_line(error: BaseException) -> str:
    text = str(error) or error.__class__.__name__
    return f"{BLOCK_MARK} {text}" if isinstance(error, Blocked) else text


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


PROFILE_PREFIX = "dy-edge-"
CREATE_NO_WINDOW = 0x08000000
CREATE_SUSPENDED = 0x00000004
PROCESS_TERMINATE = 0x0001
PROCESS_SET_QUOTA = 0x0100
PROCESS_SUSPEND_RESUME = 0x0800
PROCESS_QUERY_LIMITED_INFORMATION = 0x1000
ERROR_ACCESS_DENIED = 5
STILL_ACTIVE = 259
JOB_OBJECT_EXTENDED_LIMIT_INFORMATION = 9
JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE = 0x2000


class IoCounters(ctypes.Structure):
    _fields_ = [
        (name, ctypes.c_ulonglong)
        for name in ("read_ops", "write_ops", "other_ops", "read_bytes", "write_bytes", "other_bytes")
    ]


class JobBasicLimits(ctypes.Structure):
    _fields_ = [
        ("per_process_user_time", ctypes.c_longlong),
        ("per_job_user_time", ctypes.c_longlong),
        ("limit_flags", wintypes.DWORD),
        ("min_working_set", ctypes.c_size_t),
        ("max_working_set", ctypes.c_size_t),
        ("active_process_limit", wintypes.DWORD),
        ("affinity", ctypes.c_size_t),
        ("priority_class", wintypes.DWORD),
        ("scheduling_class", wintypes.DWORD),
    ]


class JobExtendedLimits(ctypes.Structure):
    _fields_ = [
        ("basic", JobBasicLimits),
        ("io", IoCounters),
        ("process_memory_limit", ctypes.c_size_t),
        ("job_memory_limit", ctypes.c_size_t),
        ("peak_process_memory", ctypes.c_size_t),
        ("peak_job_memory", ctypes.c_size_t),
    ]


_kernel32 = None
_ntdll = None


def kernel32():
    """kernel32 với kiểu tham số khai báo đủ. Tay cầm 64-bit mà để ctypes tự đoán kiểu sẽ bị cắt còn 32-bit."""
    global _kernel32
    if _kernel32 is None:
        lib = ctypes.WinDLL("kernel32", use_last_error=True)
        lib.OpenProcess.restype = wintypes.HANDLE
        lib.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
        lib.GetExitCodeProcess.restype = wintypes.BOOL
        lib.GetExitCodeProcess.argtypes = [wintypes.HANDLE, ctypes.POINTER(wintypes.DWORD)]
        lib.CloseHandle.restype = wintypes.BOOL
        lib.CloseHandle.argtypes = [wintypes.HANDLE]
        lib.CreateJobObjectW.restype = wintypes.HANDLE
        lib.CreateJobObjectW.argtypes = [wintypes.LPVOID, wintypes.LPCWSTR]
        lib.SetInformationJobObject.restype = wintypes.BOOL
        lib.SetInformationJobObject.argtypes = [wintypes.HANDLE, ctypes.c_int, wintypes.LPVOID, wintypes.DWORD]
        lib.AssignProcessToJobObject.restype = wintypes.BOOL
        lib.AssignProcessToJobObject.argtypes = [wintypes.HANDLE, wintypes.HANDLE]
        lib.TerminateJobObject.restype = wintypes.BOOL
        lib.TerminateJobObject.argtypes = [wintypes.HANDLE, wintypes.UINT]
        _kernel32 = lib
    return _kernel32


def ntdll():
    global _ntdll
    if _ntdll is None:
        lib = ctypes.WinDLL("ntdll")
        lib.NtResumeProcess.restype = ctypes.c_long
        lib.NtResumeProcess.argtypes = [wintypes.HANDLE]
        _ntdll = lib
    return _ntdll


class KillJob:
    """Job Windows kiểu "đóng là giết hết". Edge do msedge.exe khởi chạy rồi thoát ngay, để lại cây tiến trình
    thật không còn cha: kill hay taskkill theo tiến trình vừa chạy bỏ sót nó. Job gom cả cây, kể cả con cháu
    sinh ra sau, và chết theo tiến trình giữ tay cầm, nên Rust kill Python lúc bấm Dừng thì Edge cũng chết."""

    def __init__(self, handle) -> None:
        self.handle = handle

    @classmethod
    def create(cls) -> KillJob | None:
        if os.name != "nt":
            return None
        api = kernel32()
        handle = api.CreateJobObjectW(None, None)
        if not handle:
            return None
        limits = JobExtendedLimits()
        limits.basic.limit_flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        if not api.SetInformationJobObject(
            handle, JOB_OBJECT_EXTENDED_LIMIT_INFORMATION, ctypes.byref(limits), ctypes.sizeof(limits)
        ):
            api.CloseHandle(handle)
            return None
        return cls(handle)

    def add(self, pid: int) -> bool:
        api = kernel32()
        process = api.OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, False, pid)
        if not process:
            return False
        try:
            return bool(api.AssignProcessToJobObject(self.handle, process))
        finally:
            api.CloseHandle(process)

    def kill(self) -> None:
        if self.handle:
            kernel32().TerminateJobObject(self.handle, 1)

    def close(self) -> None:
        """Đóng tay cầm. Job còn tiến trình nào thì Windows giết nốt."""
        if self.handle:
            kernel32().CloseHandle(self.handle)
            self.handle = None


def resume_process(pid: int) -> None:
    process = kernel32().OpenProcess(PROCESS_SUSPEND_RESUME, False, pid)
    if not process:
        raise OSError(f"Không mở được tiến trình {pid} để chạy tiếp.")
    try:
        status = ntdll().NtResumeProcess(process)
    finally:
        kernel32().CloseHandle(process)
    if status != 0:
        raise OSError(f"Không chạy tiếp được tiến trình {pid} (NTSTATUS 0x{status & 0xFFFFFFFF:08X}).")


def spawn_in_job(command: list[str], **options) -> tuple[subprocess.Popen, KillJob | None]:
    """Chạy `command` trong một job riêng. Tiến trình bị giữ tạm dừng cho tới khi đã vào job, để con cháu
    sinh ra sau đó chắc chắn nằm trong job. Trả (Popen, job); job là None khi không gắn được (không phải
    Windows hoặc Windows từ chối): lúc đó tiến trình vẫn chạy bình thường, chỉ mất lớp dọn này."""
    job = KillJob.create()
    if job is None:
        return subprocess.Popen(command, **options), None
    flags = options.pop("creationflags", 0)
    proc = subprocess.Popen(command, creationflags=flags | CREATE_SUSPENDED, **options)
    try:
        joined = job.add(proc.pid)
        resume_process(proc.pid)
    except BaseException:
        proc.kill()
        job.close()
        raise
    if not joined:
        job.close()
        return proc, None
    return proc, job


def temp_root() -> Path:
    return Path(os.environ.get("TEMP") or tempfile.gettempdir())


def profile_dir(pid: int | None = None) -> Path:
    """Hồ sơ Edge tạm của một tiến trình Python. Số tiến trình nằm trong tên để quét được sau này."""
    return temp_root() / f"{PROFILE_PREFIX}{os.getpid() if pid is None else pid}"


def remove_profile(path: Path, attempts: int = 12, pause: float = 0.25) -> bool:
    """Xóa thư mục hồ sơ. Edge vừa bị kill có thể còn giữ file vài trăm mili giây nên thử lại.
    Trả False nếu thư mục vẫn còn."""
    for attempt in range(attempts):
        shutil.rmtree(path, ignore_errors=True)
        if not path.exists():
            return True
        if attempt + 1 < attempts:
            time.sleep(pause)
    return False


def process_alive(pid: int) -> bool:
    if pid <= 0:
        return False
    if os.name != "nt":
        try:
            os.kill(pid, 0)
        except ProcessLookupError:
            return False
        except PermissionError:
            return True
        return True
    api = kernel32()
    handle = api.OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, False, pid)
    if not handle:
        # Có tiến trình nhưng không mở được (quyền) vẫn là còn sống; không có thì lỗi khác.
        return ctypes.get_last_error() == ERROR_ACCESS_DENIED
    try:
        code = wintypes.DWORD()
        if not api.GetExitCodeProcess(handle, ctypes.byref(code)):
            return True
        return code.value == STILL_ACTIVE
    finally:
        api.CloseHandle(handle)


def sweep_stale_profiles(
    root: Path | None = None,
    is_alive=None,
    now: float | None = None,
    max_age: float = 6 * 3600,
) -> list[Path]:
    """Xóa thư mục dy-edge-<pid> mà tiến trình chủ đã hết (bị dừng cứng, hỏng) hoặc quá cũ (số tiến trình
    đã bị Windows dùng lại cho chương trình khác; một lần tải không bao giờ chạy lâu thế).
    Không đụng thư mục của chính tiến trình này, và chỉ đụng thư mục đúng tên. Trả danh sách đã xóa."""
    root = root or temp_root()
    is_alive = is_alive or process_alive
    now = time.time() if now is None else now
    removed: list[Path] = []
    try:
        entries = list(root.iterdir())
    except OSError:
        return removed
    for entry in entries:
        match = re.fullmatch(re.escape(PROFILE_PREFIX) + r"(\d+)", entry.name)
        if not match or entry.is_symlink() or not entry.is_dir():
            continue
        pid = int(match.group(1))
        if pid == os.getpid():
            continue
        try:
            age = now - entry.stat().st_mtime
        except OSError:
            continue
        if is_alive(pid) and age <= max_age:
            continue
        if remove_profile(entry, attempts=2, pause=0.1):
            removed.append(entry)
    return removed


def kill_tree(proc: subprocess.Popen) -> None:
    """Kill cả cây tiến trình. Kill mỗi tiến trình chính thì các tiến trình con của Edge (GPU, mạng) ở lại giữ file."""
    if proc.poll() is None and os.name == "nt":
        try:
            subprocess.run(
                ["taskkill", "/PID", str(proc.pid), "/T", "/F"],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                creationflags=CREATE_NO_WINDOW,
                timeout=15,
                check=False,
            )
        except (OSError, subprocess.SubprocessError):
            pass
    try:
        proc.kill()
    except OSError:
        pass
    try:
        proc.wait(timeout=10)
    except subprocess.TimeoutExpired:
        pass


class Browser:
    """Edge headless đang chạy cùng mọi thứ phải dọn khi xong: cây tiến trình và thư mục hồ sơ tạm."""

    def __init__(self, proc: subprocess.Popen, job: KillJob | None, profile: Path) -> None:
        self.proc = proc
        self.job = job
        self.profile = profile

    def close(self) -> None:
        if self.job:
            self.job.kill()
            self.job.close()
            try:
                self.proc.wait(timeout=5)  # thu dọn tiến trình khởi chạy, thường đã thoát từ lâu
            except subprocess.TimeoutExpired:
                pass
        else:
            kill_tree(self.proc)
        remove_profile(self.profile)


def session(page_url: str) -> tuple[Browser, Cdp, dict]:
    binary = edge_path()
    if not binary:
        raise RuntimeError("Chưa thấy Microsoft Edge. Tải Douyin cần Edge.")
    profile = profile_dir()
    remove_profile(profile)
    profile.mkdir(parents=True, exist_ok=True)
    port = free_port()
    try:
        proc, job = spawn_in_job(
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
    except OSError:
        remove_profile(profile)
        raise
    browser = Browser(proc, job, profile)
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
        return browser, cdp, screen
    except BaseException:
        browser.close()
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
        f"{DETAIL_URL}?{signed}",
        headers={
            "User-Agent": screen["ua"],
            "Referer": f"{PAGE}video/{screen['aweme']}",
            "Accept": "application/json, text/plain, */*",
            "Accept-Language": "zh-CN,zh;q=0.9",
            "Cookie": screen["cookie"],
            **extra,
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=25) as response:
            body = response.read()
    except urllib.error.HTTPError as error:
        error.close()
        if error.code in BLOCKING_CODES:
            raise Blocked(f"Douyin từ chối yêu cầu (HTTP {error.code}).") from error
        raise RuntimeError(f"Douyin trả lỗi HTTP {error.code}.") from error
    text = body.decode("utf-8", "replace").strip()
    if not text:
        raise Blocked("Douyin trả về trang trống, không có dữ liệu video.")
    try:
        data = json.loads(text)
    except ValueError as error:
        raise Blocked("Douyin trả về trang không phải dữ liệu video.") from error
    detail = data.get("aweme_detail") if isinstance(data, dict) else None
    if not isinstance(detail, dict) or not play_urls(detail):
        raise RuntimeError("Douyin không trả địa chỉ video. Video có thể đã bị xóa hoặc ẩn.")
    return detail


def mp4_complete(path: Path) -> bool:
    """File mp4 đủ khi các hộp (box) nối vừa khít tới cuối file và có cả moov lẫn mdat.

    Tải dở thì hộp cuối khai báo dài hơn phần còn lại của file, hoặc thiếu moov. Chỉ đọc đầu mỗi hộp.
    Rust có bản cùng luật: `mp4_complete` trong src/pipeline/mp4_complete.rs. Sửa bên này thì sửa cả bên kia.
    """
    try:
        size = path.stat().st_size
        seen: set[bytes] = set()
        offset = 0
        with path.open("rb") as handle:
            while offset < size:
                handle.seek(offset)
                head = handle.read(16)
                if len(head) < 8:
                    return False
                length = int.from_bytes(head[:4], "big")
                header = 8
                if length == 1:
                    if len(head) < 16:
                        return False
                    length = int.from_bytes(head[8:16], "big")
                    header = 16
                elif length == 0:
                    length = size - offset
                if length < header or offset + length > size:
                    return False
                seen.add(head[4:8])
                offset += length
        return offset == size and b"moov" in seen and b"mdat" in seen
    except OSError:
        return False


def part_path(dest: Path) -> Path:
    return dest.with_name(dest.name + ".part")


def reusable_video(folder: Path, aweme: str) -> Path | None:
    """File đã tải đủ cho video này. File cụt của lần dừng giữa chừng không tính."""
    suffix = f"[{aweme}].mp4"
    for path in sorted(folder.iterdir()):
        if path.name.endswith(suffix) and mp4_complete(path):
            return path
    return None


def drop_stale_parts(folder: Path, aweme: str) -> None:
    """Xóa file tạm còn sót của video này (lần trước bị dừng; tên có thể khác vì tiêu đề đổi)."""
    for path in folder.iterdir():
        if path.name.endswith(f"[{aweme}].mp4.part"):
            try:
                path.unlink()
            except OSError:
                pass


def copy_to_part(response, part: Path, total_n: int | None) -> int:
    started = time.time()
    written = 0
    last = 0.0
    with part.open("wb") as handle:
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
                total_text = str(total_n) if total_n else "NA"
                eta_text = str(int(eta)) if eta and eta >= 0 else "NA"
                print(
                    f"DLP downloading {written} {total_text} NA {speed:.1f} {eta_text}",
                    file=sys.stderr,
                    flush=True,
                )
                last = now
    return written


def save_video(urls: list[str], dest: Path, screen: dict) -> None:
    """Ghi vào <tên>.part, đủ rồi mới đổi sang tên cuối. Tên cuối không bao giờ giữ file dở."""
    headers = {
        "User-Agent": screen["ua"],
        "Referer": PAGE,
        "Cookie": screen["cookie"],
    }
    part = part_path(dest)
    reason = "Douyin không đưa địa chỉ nào tải được."
    for url in urls:
        request = urllib.request.Request(url, headers=headers)
        try:
            with urllib.request.urlopen(request, timeout=30) as response:
                total = response.headers.get("Content-Length")
                total_n = int(total) if total and total.isdigit() else None
                written = copy_to_part(response, part, total_n)
        except (OSError, http.client.HTTPException) as error:
            part.unlink(missing_ok=True)
            reason = f"Máy chủ video không trả file ({error})."
            continue
        # Python không báo lỗi khi thân ngắn hơn Content-Length, nên phải tự so.
        if (total_n is None or written == total_n) and mp4_complete(part):
            os.replace(part, dest)
            print(f"DLP finished {written} {written} NA NA 0", file=sys.stderr, flush=True)
            return
        part.unlink(missing_ok=True)
        reason = f"File nhận về chưa đủ ({written} byte)."
    raise RuntimeError(f"Tải file video thất bại. {reason}")


def download(page_url: str, folder: str) -> Path:
    out = Path(folder)
    if not out.is_dir():
        raise RuntimeError("Folder kết quả chưa có.")
    try:
        sweep_stale_profiles()
    except Exception:
        pass  # dọn lần trước còn sót chỉ là việc phụ, không được chặn lần tải này
    browser, _cdp, screen = session(page_url)
    try:
        existing = reusable_video(out, screen["aweme"])
        if existing:
            return existing
        drop_stale_parts(out, screen["aweme"])
        loaded = detail_for(screen)
        title = str(loaded.get("desc") or "douyin")
        dest = out / windows_name(title, screen["aweme"])
        save_video(play_urls(loaded), dest, screen)
        return dest
    finally:
        browser.close()


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


def run() -> None:
    try:
        main()
    except Exception as error:
        print(error_line(error), file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    run()
