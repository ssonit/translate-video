"""Test cho các hàm tải file của src-tauri/douyin/fetch.py.

Chạy: python -m unittest discover -s scripts -p "douyin_*_test.py"
"""

import contextlib
import io
import json
import pathlib
import sys
import tempfile
import threading
import time
import unittest
from unittest import mock

from douyin_testkit import Site, box, load_fetch, make_mp4, send

fetch = load_fetch()

AWEME = "7691382497195625743"
SCREEN = {"ua": "test-agent", "cookie": ""}


def detail_screen():
    """Đủ trường để detail_query ký request, không cần Edge."""
    info = fetch.build_browser_info(
        inner_width=1280, inner_height=720, outer_width=1280, outer_height=800,
        avail_width=1920, avail_height=1040, screen_width=1920, screen_height=1080, platform="Win32",
    )
    return {
        "ua": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/139.0.0.0 Safari/537.36",
        "info": info, "sw": 1920, "sh": 1080, "cores": 8, "mem": 8,
        "uifid": "test-uifid", "verify": "", "cookie": "", "aweme": AWEME,
    }


def quiet_save(urls, dest):
    """save_video in tiến độ ra stderr; test không cần xem."""
    with contextlib.redirect_stderr(io.StringIO()):
        fetch.save_video(urls, dest, SCREEN)


class TempFolder(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self._tmp.cleanup)
        self.folder = pathlib.Path(self._tmp.name)

    def files(self):
        return sorted(path.name for path in self.folder.iterdir())


class Mp4Complete(TempFolder):
    """Một file mp4 chỉ được coi là xong khi các hộp nối vừa khít tới cuối file."""

    def write(self, data: bytes) -> pathlib.Path:
        path = self.folder / "v.mp4"
        path.write_bytes(data)
        return path

    def test_whole_file_is_complete(self):
        self.assertTrue(fetch.mp4_complete(self.write(make_mp4())))

    def test_file_cut_inside_the_last_box_is_not_complete(self):
        # Cụt giữa mdat nhưng vẫn lớn hơn 100 KB: ngưỡng cũ của fetch.py cho qua.
        self.assertFalse(fetch.mp4_complete(self.write(make_mp4()[:150_000])))

    def test_file_without_mdat_is_not_complete(self):
        # Có ftyp và moov mà chưa có dữ liệu hình: tải dở ngay sau phần đầu.
        self.assertFalse(fetch.mp4_complete(self.write(make_mp4(0)[:-8])))

    def test_file_cut_exactly_before_moov_is_not_complete(self):
        # mp4 không faststart: moov nằm sau mdat. Cắt đúng ở ranh giới hộp thì các hộp vẫn khít nhưng không phát được.
        whole = make_mp4()
        moov_start = whole.index(b"moov") - 4
        without_moov = whole[:moov_start] + whole[moov_start + 8 + 100 :]
        self.assertFalse(fetch.mp4_complete(self.write(without_moov)))

    def test_error_page_is_not_complete(self):
        self.assertFalse(fetch.mp4_complete(self.write(b"<html><body>403</body></html>" * 20)))

    def test_empty_file_is_not_complete(self):
        self.assertFalse(fetch.mp4_complete(self.write(b"")))

    def test_missing_file_is_not_complete(self):
        self.assertFalse(fetch.mp4_complete(self.folder / "khong-co.mp4"))

    def test_last_box_with_size_zero_runs_to_the_end(self):
        whole = make_mp4(5_000)
        mdat_start = whole.index(b"mdat") - 4
        zero_size = whole[:mdat_start] + (0).to_bytes(4, "big") + whole[mdat_start + 4 :]
        self.assertTrue(fetch.mp4_complete(self.write(zero_size)))

    def test_box_smaller_than_its_own_header_is_not_complete(self):
        # Hộp khai báo dài 4 byte, ngắn hơn cả phần đầu 8 byte. Nếu cứ nhảy 4 byte thì "hộp kế tiếp" nằm chồng
        # lên chỗ kiểu hộp và trông hợp lệ, file bị tính là đủ.
        broken = (4).to_bytes(4, "big") + (8).to_bytes(4, "big") + b"free"
        self.assertFalse(fetch.mp4_complete(self.write(make_mp4() + broken)))

    def test_box_with_64_bit_size_is_read(self):
        payload = b"\x22" * 5_000
        big = (1).to_bytes(4, "big") + b"mdat" + (16 + len(payload)).to_bytes(8, "big") + payload
        head = make_mp4(0)[: -8]  # ftyp + moov, bỏ mdat rỗng
        whole = head + big
        self.assertTrue(fetch.mp4_complete(self.write(whole)))
        self.assertFalse(fetch.mp4_complete(self.write(whole[:-1])))


class SaveVideo(TempFolder):
    """save_video chỉ để lại tên cuối khi file đã đủ."""

    def setUp(self):
        super().setUp()
        self.dest = self.folder / f"Tiêu đề [{AWEME}].mp4"
        self.full = make_mp4()

    def test_whole_download_lands_under_its_final_name(self):
        with Site() as site:
            site.routes["/ok.mp4"] = lambda h: send(h, self.full)
            quiet_save([site.base + "/ok.mp4"], self.dest)
        self.assertEqual(self.dest.read_bytes(), self.full)
        self.assertEqual(self.files(), [self.dest.name])

    def test_body_shorter_than_content_length_is_not_kept(self):
        # Python không báo lỗi khi thân ngắn hơn Content-Length (http.client.read(amt)), nên phải tự so.
        cut = self.full[: len(self.full) // 2]
        with Site() as site:
            site.routes["/cut.mp4"] = lambda h: send(h, cut, length=len(self.full))
            with self.assertRaises(RuntimeError):
                quiet_save([site.base + "/cut.mp4"], self.dest)
        self.assertEqual(self.files(), [])

    def test_body_cut_exactly_between_two_boxes_is_caught_by_content_length(self):
        # Cụt đúng ranh giới hộp (vd. mp4 phân đoạn, hết một mdat chưa tới moof kế): các hộp vẫn khít và đủ
        # moov, mdat, nên chỉ Content-Length cho biết còn thiếu.
        declared = self.full + box(b"moof", bytes(50)) + box(b"mdat", b"m" * 5_000)
        with Site() as site:
            site.routes["/cut.mp4"] = lambda h: send(h, self.full, length=len(declared))
            with self.assertRaises(RuntimeError):
                quiet_save([site.base + "/cut.mp4"], self.dest)
        self.assertEqual(self.files(), [])

    def test_cut_body_without_content_length_is_caught_by_the_file_structure(self):
        cut = self.full[: len(self.full) // 2]
        with Site() as site:
            site.routes["/cut.mp4"] = lambda h: send(h, cut, length=None)
            with self.assertRaises(RuntimeError):
                quiet_save([site.base + "/cut.mp4"], self.dest)
        self.assertEqual(self.files(), [])

    def test_next_address_is_tried_after_a_cut_one(self):
        cut = self.full[: len(self.full) // 2]
        with Site() as site:
            site.routes["/cut.mp4"] = lambda h: send(h, cut, length=len(self.full))
            site.routes["/ok.mp4"] = lambda h: send(h, self.full)
            quiet_save([site.base + "/cut.mp4", site.base + "/ok.mp4"], self.dest)
        self.assertEqual(self.dest.read_bytes(), self.full)
        self.assertEqual(self.files(), [self.dest.name])

    def test_error_page_is_not_kept(self):
        with Site() as site:
            site.routes["/403.mp4"] = lambda h: send(h, b"<html>blocked</html>" * 2_000)
            with self.assertRaises(RuntimeError):
                quiet_save([site.base + "/403.mp4"], self.dest)
        self.assertEqual(self.files(), [])

    def test_final_name_stays_empty_until_the_file_is_whole(self):
        # Dừng giữa chừng (Rust kill Python) chỉ được để lại file tạm, không để lại tên cuối.
        # fetch.py đọc từng khúc 256 KB nên file phải đủ lớn để vài khúc đã xuống đĩa trước chỗ chặn.
        full = make_mp4(1_000_000)
        paused_at = 600_000
        reached, release = threading.Event(), threading.Event()

        def slow(handler):
            handler.send_response(200)
            handler.send_header("Content-Length", str(len(full)))
            handler.end_headers()
            handler.wfile.write(full[:paused_at])
            handler.wfile.flush()
            reached.set()
            release.wait(15)
            handler.wfile.write(full[paused_at:])

        def on_disk():
            return sum(path.stat().st_size for path in self.folder.iterdir())

        with Site() as site:
            site.routes["/slow.mp4"] = slow
            worker = threading.Thread(target=quiet_save, args=([site.base + "/slow.mp4"], self.dest))
            worker.start()
            try:
                self.assertTrue(reached.wait(10))
                deadline = time.time() + 10
                while time.time() < deadline and on_disk() == 0:
                    time.sleep(0.02)
                self.assertGreater(on_disk(), 0, "chưa byte nào xuống đĩa, test không đo được gì")
                self.assertFalse(self.dest.exists(), "một phần file đã nằm dưới tên cuối")
            finally:
                release.set()
                worker.join(15)
        self.assertEqual(self.dest.read_bytes(), full)

    def test_an_older_cut_file_of_the_same_name_is_replaced(self):
        # Bảo vệ lúc đổi tên: Windows không cho os.rename đè lên file có sẵn.
        self.dest.write_bytes(self.full[:150_000])
        with Site() as site:
            site.routes["/ok.mp4"] = lambda h: send(h, self.full)
            quiet_save([site.base + "/ok.mp4"], self.dest)
        self.assertEqual(self.dest.read_bytes(), self.full)


class ReuseDecision(TempFolder):
    """Lần sau chỉ dùng lại file đã đủ; file cụt của lần dừng giữa chừng không được coi là đã tải."""

    def test_whole_file_of_this_video_is_reused(self):
        path = self.folder / f"Tên [{AWEME}].mp4"
        path.write_bytes(make_mp4())
        self.assertEqual(fetch.reusable_video(self.folder, AWEME), path)

    def test_cut_file_of_this_video_is_not_reused(self):
        (self.folder / f"Tên [{AWEME}].mp4").write_bytes(make_mp4()[:150_000])
        self.assertIsNone(fetch.reusable_video(self.folder, AWEME))

    def test_whole_file_of_another_video_is_not_mistaken(self):
        (self.folder / "Tên [7000000000000000001].mp4").write_bytes(make_mp4())
        self.assertIsNone(fetch.reusable_video(self.folder, AWEME))

    def test_a_cut_file_does_not_hide_a_whole_one(self):
        (self.folder / f"A [{AWEME}].mp4").write_bytes(make_mp4()[:150_000])
        good = self.folder / f"B [{AWEME}].mp4"
        good.write_bytes(make_mp4())
        self.assertEqual(fetch.reusable_video(self.folder, AWEME), good)

    def test_leftover_part_files_of_this_video_are_dropped_and_others_kept(self):
        (self.folder / f"Tên cũ [{AWEME}].mp4.part").write_bytes(b"x" * 10)
        keep_other = self.folder / "Khác [7000000000000000001].mp4.part"
        keep_other.write_bytes(b"y")
        keep_whole = self.folder / f"Tên [{AWEME}].mp4"
        keep_whole.write_bytes(make_mp4())
        fetch.drop_stale_parts(self.folder, AWEME)
        self.assertEqual(self.files(), sorted([keep_other.name, keep_whole.name]))


class DetailAnswers(unittest.TestCase):
    """Douyin từ chối (bị chặn) khác với video không còn: chỉ chuyện đầu làm app nghỉ."""

    def ask(self, route):
        with Site() as site:
            site.routes["/detail/"] = route
            with mock.patch.object(fetch, "DETAIL_URL", site.base + "/detail/", create=True):
                return fetch.detail_for(detail_screen())

    def test_http_403_is_a_block(self):
        route = lambda h: send(h, b"Blocked by ArgusSecurityPlugin Signature Not Found", status=403)
        with self.assertRaises(fetch.Blocked) as caught:
            self.ask(route)
        self.assertIn("403", str(caught.exception))

    def test_http_429_is_a_block(self):
        with self.assertRaises(fetch.Blocked) as caught:
            self.ask(lambda h: send(h, b"slow down", status=429))
        self.assertIn("429", str(caught.exception))

    def test_empty_answer_is_a_block(self):
        with self.assertRaises(fetch.Blocked):
            self.ask(lambda h: send(h, b""))

    def test_a_page_instead_of_json_is_a_block(self):
        with self.assertRaises(fetch.Blocked):
            self.ask(lambda h: send(h, b"<html><body>captcha</body></html>"))

    def test_json_without_a_video_is_unavailable_not_a_block(self):
        body = json.dumps({"status_code": 0, "aweme_detail": None}).encode()
        with self.assertRaises(RuntimeError) as caught:
            self.ask(lambda h: send(h, body))
        self.assertNotIsInstance(caught.exception, fetch.Blocked)

    def test_server_error_is_an_ordinary_failure_not_a_block(self):
        with self.assertRaises(RuntimeError) as caught:
            self.ask(lambda h: send(h, b"oops", status=500))
        self.assertNotIsInstance(caught.exception, fetch.Blocked)

    def test_a_good_answer_returns_the_detail(self):
        detail = {
            "desc": "Tiêu đề",
            "video": {"bit_rate": [{"height": 1080, "play_addr": {"url_list": ["http://127.0.0.1/a.mp4"]}}]},
        }
        body = json.dumps({"aweme_detail": detail}).encode()
        self.assertEqual(self.ask(lambda h: send(h, body)), detail)


def text_stream():
    """Luồng chữ có `reconfigure`, như stdout/stderr thật mà main() gọi."""
    raw = io.BytesIO()
    return io.TextIOWrapper(raw, encoding="utf-8", write_through=True), raw


class ErrorLine(unittest.TestCase):
    """Dòng lỗi in ra stderr là hợp đồng với Rust: dòng bị chặn mang dấu để app nhận ra."""

    def test_a_block_carries_the_mark_the_app_looks_for(self):
        line = fetch.error_line(fetch.Blocked("Douyin từ chối yêu cầu (HTTP 403)."))
        self.assertEqual(line, "[douyin-blocked] Douyin từ chối yêu cầu (HTTP 403).")

    def test_other_errors_are_printed_as_they_are(self):
        line = fetch.error_line(RuntimeError("Không đọc được mã video từ link Douyin."))
        self.assertEqual(line, "Không đọc được mã video từ link Douyin.")

    def test_run_prints_the_mark_and_exits_with_1_for_a_block(self):
        out, _ = text_stream()
        err, raw = text_stream()
        with mock.patch.object(sys, "argv", ["fetch.py", "https://v.douyin.com/x/", "folder"]), \
                mock.patch.object(sys, "stdout", out), mock.patch.object(sys, "stderr", err), \
                mock.patch.object(fetch, "download", side_effect=fetch.Blocked("Douyin từ chối yêu cầu (HTTP 403).")):
            with self.assertRaises(SystemExit) as caught:
                fetch.run()
        self.assertEqual(caught.exception.code, 1)
        self.assertEqual(raw.getvalue().decode("utf-8").strip(), "[douyin-blocked] Douyin từ chối yêu cầu (HTTP 403).")


if __name__ == "__main__":
    unittest.main()
