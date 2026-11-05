"""Đồ dùng chung cho test của src-tauri/douyin/fetch.py.

Test không bao giờ được hỏi Douyin thật: `load_fetch` chặn mọi kết nối và tra DNS ra ngoài máy,
chỉ cho 127.0.0.1. Máy chủ giả (`Site`) là máy chủ HTTP thật trên cổng tự chọn.
"""

import http.server
import importlib.util
import pathlib
import socket
import threading

ROOT = pathlib.Path(__file__).resolve().parents[1]
FETCH_PATH = ROOT / "src-tauri" / "douyin" / "fetch.py"

_LOCAL = {"127.0.0.1", "::1", "localhost"}
_real_connect = socket.socket.connect
_real_getaddrinfo = socket.getaddrinfo


def _local_connect(self, address):
    host = address[0] if isinstance(address, tuple) else address
    if host not in _LOCAL:
        raise OSError(f"test không được nối ra ngoài máy: {address!r}")
    return _real_connect(self, address)


def _local_lookup(host, *args, **kwargs):
    if host not in _LOCAL and host not in (None, ""):
        raise socket.gaierror(f"test không được tra DNS ra ngoài máy: {host!r}")
    return _real_getaddrinfo(host, *args, **kwargs)


def block_outside_network():
    socket.socket.connect = _local_connect
    socket.getaddrinfo = _local_lookup


def load_fetch():
    """Nạp fetch.py dưới tên riêng, sau khi đã chặn mạng ngoài máy."""
    block_outside_network()
    spec = importlib.util.spec_from_file_location("douyin_fetch", FETCH_PATH)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def box(kind: bytes, payload: bytes) -> bytes:
    return (8 + len(payload)).to_bytes(4, "big") + kind + payload


def make_mp4(mdat_size: int = 300_000) -> bytes:
    """Một file mp4 đủ hộp: ftyp, moov rồi mdat. Mặc định lớn hơn 100 KB, ngưỡng cũ của fetch.py."""
    return box(b"ftyp", b"isom\0\0\2\0isomiso2") + box(b"moov", b"\0" * 100) + box(b"mdat", b"\x11" * mdat_size)


def send(handler, body: bytes, status: int = 200, length="auto", headers=()):
    """Trả một phản hồi. length="auto": ghi Content-Length đúng; None: không ghi; số: ghi số đó."""
    handler.send_response(status)
    if length == "auto":
        length = len(body)
    if length is not None:
        handler.send_header("Content-Length", str(length))
    for name, value in headers:
        handler.send_header(name, value)
    handler.end_headers()
    handler.wfile.write(body)
    handler.wfile.flush()


class Site:
    """Máy chủ HTTP nghe 127.0.0.1. `routes` ánh xạ đường dẫn (không kèm query) sang hàm nhận handler."""

    def __init__(self):
        self.routes = {}
        site = self

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_GET(self):
                route = site.routes.get(self.path.split("?", 1)[0])
                if route is None:
                    self.send_error(404)
                    return
                route(self)

            def log_message(self, *args):
                pass

        self.server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.server.daemon_threads = True
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)

    @property
    def base(self) -> str:
        return f"http://127.0.0.1:{self.server.server_port}"

    def __enter__(self):
        self.thread.start()
        return self

    def __exit__(self, *exc):
        self.server.shutdown()
        self.server.server_close()
