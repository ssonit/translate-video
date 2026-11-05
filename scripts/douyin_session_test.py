"""Test phần dọn Edge và thư mục tạm của src-tauri/douyin/fetch.py.

Các ca "Edge thật" dùng Microsoft Edge headless thật nhưng Douyin giả trên 127.0.0.1 (trang cấp cookie
khách, API chi tiết, file mp4). Không ca nào hỏi Douyin thật. Thiếu Edge hoặc không phải Windows thì bỏ qua.

Chạy: python -m unittest discover -s scripts -p "douyin_*_test.py"
"""

import contextlib
import io
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from unittest import mock

from douyin_testkit import FETCH_PATH, Site, load_fetch, make_mp4, send

fetch = load_fetch()

AWEME = "7691382497195625743"
TITLE = "Tiêu đề thử"
ON_WINDOWS = os.name == "nt"
HAS_EDGE = ON_WINDOWS and fetch.edge_path() is not None


def wait_until(check, seconds: float, step: float = 0.1) -> bool:
    deadline = time.time() + seconds
    while time.time() < deadline:
        if check():
            return True
        time.sleep(step)
    return check()


def edge_pids_using(folder: pathlib.Path) -> list[int]:
    """Các msedge.exe có dòng lệnh chứa đường dẫn này (chỉ Edge của test, không đụng Edge của người dùng)."""
    script = (
        "Get-CimInstance Win32_Process -Filter \"Name='msedge.exe'\" | "
        "Where-Object { $_.CommandLine -and $_.CommandLine.Contains($env:SCRATCH_DIR) } | "
        "ForEach-Object { $_.ProcessId }"
    )
    result = subprocess.run(
        ["powershell", "-NoProfile", "-NonInteractive", "-Command", script],
        capture_output=True, text=True, timeout=90, env={**os.environ, "SCRATCH_DIR": str(folder)},
    )
    return [int(token) for token in result.stdout.split() if token.isdigit()]


def kill_pids(pids) -> None:
    for pid in pids:
        subprocess.run(["taskkill", "/PID", str(pid), "/T", "/F"], capture_output=True)


class Scratch(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory(prefix="dy-test-")
        self.addCleanup(self._tmp.cleanup)
        self.root = pathlib.Path(self._tmp.name)


@unittest.skipUnless(ON_WINDOWS, "thư mục Edge và khóa file kiểu Windows")
class Profiles(Scratch):
    """Thư mục hồ sơ Edge tạm: xóa được, và quét được cái do tiến trình đã chết để lại."""

    def make_profile(self, name: str) -> pathlib.Path:
        profile = self.root / name
        (profile / "Default" / "Cache").mkdir(parents=True)
        (profile / "Default" / "Cache" / "data_0").write_bytes(b"x" * 1024)
        (profile / "Local State").write_bytes(b"{}")
        return profile

    def test_remove_profile_deletes_the_whole_tree(self):
        profile = self.make_profile("dy-edge-100")
        self.assertTrue(fetch.remove_profile(profile))
        self.assertFalse(profile.exists())

    def test_remove_profile_reports_failure_while_a_file_is_locked(self):
        profile = self.make_profile("dy-edge-100")
        held = (profile / "Default" / "Cache" / "data_0").open("rb")
        try:
            self.assertFalse(fetch.remove_profile(profile, attempts=2, pause=0.01))
            self.assertTrue(profile.exists())
        finally:
            held.close()
        self.assertTrue(fetch.remove_profile(profile, attempts=2, pause=0.01))
        self.assertFalse(profile.exists())

    def test_remove_profile_keeps_trying_until_a_lock_is_released(self):
        # Edge vừa bị kill giữ file thêm một lúc; xóa ngay lần đầu thường chưa được.
        profile = self.make_profile("dy-edge-100")
        held = (profile / "Default" / "Cache" / "data_0").open("rb")
        self.addCleanup(held.close)
        timer = threading.Timer(0.4, held.close)
        timer.start()
        self.addCleanup(timer.cancel)
        self.assertTrue(fetch.remove_profile(profile, attempts=30, pause=0.1))
        self.assertFalse(profile.exists())

    def test_sweep_removes_profiles_of_finished_processes_and_keeps_live_ones(self):
        dead = self.make_profile("dy-edge-111")
        alive = self.make_profile("dy-edge-222")
        removed = fetch.sweep_stale_profiles(self.root, is_alive=lambda pid: pid == 222)
        self.assertEqual(removed, [dead])
        self.assertFalse(dead.exists())
        self.assertTrue(alive.exists())

    def test_sweep_never_removes_the_profile_of_this_very_process(self):
        mine = self.make_profile(f"dy-edge-{os.getpid()}")
        self.assertEqual(fetch.sweep_stale_profiles(self.root, is_alive=lambda pid: False), [])
        self.assertTrue(mine.exists())

    def test_sweep_removes_a_very_old_profile_even_if_its_pid_looks_alive(self):
        # Số tiến trình bị Windows dùng lại cho chương trình khác: thư mục vẫn là rác.
        old = self.make_profile("dy-edge-333")
        now = time.time()
        os.utime(old, (now - 7 * 3600, now - 7 * 3600))
        young = self.make_profile("dy-edge-444")
        removed = fetch.sweep_stale_profiles(self.root, is_alive=lambda pid: True, now=now)
        self.assertEqual(removed, [old])
        self.assertTrue(young.exists())

    def test_sweep_ignores_other_names_and_plain_files(self):
        keep = [
            self.make_profile("dy-edge-abc"),
            self.make_profile("edge-profile-5"),
            self.make_profile("dy-edge-"),
            self.make_profile("dy-edge-123-saved"),
        ]
        plain = self.root / "dy-edge-777"
        plain.write_bytes(b"not a folder")
        self.assertEqual(fetch.sweep_stale_profiles(self.root, is_alive=lambda pid: False), [])
        self.assertTrue(all(path.exists() for path in keep + [plain]))

    def test_process_alive_tells_running_from_finished(self):
        self.assertTrue(fetch.process_alive(os.getpid()))
        done = subprocess.Popen([sys.executable, "-c", "pass"])
        done.wait()
        self.assertFalse(fetch.process_alive(done.pid))


SLEEPER = "import time; time.sleep(60)"
PARENT_PLAIN = f"""
import subprocess, sys, time
child = subprocess.Popen([sys.executable, "-c", {SLEEPER!r}])
print(child.pid, flush=True)
time.sleep(60)
"""
# Giống msedge.exe trên máy này: tiến trình được chạy mở tiến trình thật rồi thoát ngay, để con mồ côi.
LAUNCHER = f"""
import subprocess, sys
child = subprocess.Popen([sys.executable, "-c", {SLEEPER!r}])
print(child.pid, flush=True)
"""
PARENT_JOBBED = f"""
import importlib.util, sys, time
spec = importlib.util.spec_from_file_location("douyin_fetch", sys.argv[1])
fetch = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fetch)
proc, job = fetch.spawn_in_job([sys.executable, "-c", {LAUNCHER!r}], stdout=-1, text=True)
print("jobbed" if job else "unjobbed", flush=True)
print(proc.stdout.readline().strip(), flush=True)
time.sleep(60)
"""
SAY_HI = "print('hi')"


@unittest.skipUnless(ON_WINDOWS, "cây tiến trình và job object của Windows")
class ProcessTree(unittest.TestCase):
    def spawn(self, code, *args):
        parent = subprocess.Popen([sys.executable, "-c", code, *args], stdout=subprocess.PIPE, text=True)
        self.addCleanup(parent.stdout.close)
        self.addCleanup(lambda: kill_pids([parent.pid]))
        return parent

    def launch(self, code):
        """Chạy `code` trong job như fetch.py chạy Edge. Trả (tiến trình được chạy, job, pid của con nó in ra)."""
        proc, job = fetch.spawn_in_job([sys.executable, "-c", code], stdout=subprocess.PIPE, text=True)
        self.addCleanup(proc.stdout.close)
        self.addCleanup(lambda: kill_pids([proc.pid]))
        self.assertIsNotNone(job, "không tạo được job")
        self.addCleanup(job.close)
        child = int(proc.stdout.readline())
        self.addCleanup(lambda: kill_pids([child]))
        return proc, job, child

    def test_kill_tree_ends_the_children_too(self):
        parent = self.spawn(PARENT_PLAIN)
        child = int(parent.stdout.readline())
        self.addCleanup(lambda: kill_pids([child]))
        self.assertTrue(fetch.process_alive(child))
        fetch.kill_tree(parent)
        self.assertTrue(wait_until(lambda: not fetch.process_alive(child), 5), "tiến trình con còn sống")
        self.assertFalse(fetch.process_alive(parent.pid))

    def test_job_kill_reaches_a_child_whose_launcher_has_already_exited(self):
        # Đúng ca của Edge: kill hay taskkill theo tiến trình được chạy không với tới con mồ côi, job thì có.
        proc, job, child = self.launch(LAUNCHER)
        proc.wait()
        self.assertFalse(fetch.process_alive(proc.pid))
        self.assertTrue(fetch.process_alive(child), "con phải còn chạy sau khi tiến trình được chạy đã thoát")
        job.kill()
        self.assertTrue(wait_until(lambda: not fetch.process_alive(child), 5), "tiến trình con còn sống")

    def test_children_die_with_their_parent_even_when_it_is_killed_hard(self):
        # Rust chỉ kill Python khi bấm Dừng. Edge phải chết theo, không ở lại mồ côi.
        parent = self.spawn(PARENT_JOBBED, str(FETCH_PATH))
        self.assertEqual(parent.stdout.readline().strip(), "jobbed")
        child = int(parent.stdout.readline())
        self.addCleanup(lambda: kill_pids([child]))
        self.assertTrue(fetch.process_alive(child))
        parent.kill()
        parent.wait()
        self.assertTrue(wait_until(lambda: not fetch.process_alive(child), 5), "tiến trình con còn sống")

    def test_process_does_not_start_running_before_it_has_joined_the_job(self):
        # Con cháu chỉ chắc chắn nằm trong job nếu tiến trình chưa chạy được dòng nào trước khi vào job.
        with tempfile.TemporaryDirectory(prefix="dy-test-") as folder:
            marker = pathlib.Path(folder) / "started"
            real_add = fetch.KillJob.add
            seen = []

            def slow_add(job, pid):
                time.sleep(1.5)  # dư để một tiến trình đang chạy kịp ghi dấu
                seen.append(marker.exists())
                return real_add(job, pid)

            with mock.patch.object(fetch.KillJob, "add", slow_add):
                proc, job = fetch.spawn_in_job([sys.executable, "-c", f"open({str(marker)!r}, 'w').close()"])
            self.addCleanup(lambda: kill_pids([proc.pid]))
            if job:
                self.addCleanup(job.close)
            proc.wait(timeout=10)
            self.assertEqual(seen, [False], "tiến trình đã chạy trước khi vào job")
            self.assertTrue(marker.exists(), "tiến trình không được chạy tiếp sau khi vào job")

    def closed_browser(self, with_job):
        """Dựng một Browser quanh tiến trình ngủ (không phải Edge), đóng nó, trả (tiến trình, thư mục hồ sơ)."""
        folder = tempfile.TemporaryDirectory(prefix="dy-test-")
        self.addCleanup(folder.cleanup)
        profile = pathlib.Path(folder.name) / "dy-edge-1"
        (profile / "Default").mkdir(parents=True)
        (profile / "Default" / "Cookies").write_bytes(b"x" * 100)
        if with_job:
            proc, job = fetch.spawn_in_job([sys.executable, "-c", SLEEPER])
            self.assertIsNotNone(job, "không tạo được job")
        else:
            proc, job = subprocess.Popen([sys.executable, "-c", SLEEPER]), None
        self.addCleanup(lambda: kill_pids([proc.pid]))
        fetch.Browser(proc, job, profile).close()
        return proc, profile

    def test_closing_a_browser_ends_its_process_reaps_it_and_removes_the_profile(self):
        proc, profile = self.closed_browser(with_job=True)
        self.assertFalse(fetch.process_alive(proc.pid))
        self.assertIsNotNone(proc.returncode, "tiến trình chưa được thu dọn (Popen vẫn tưởng còn chạy)")
        self.assertFalse(profile.exists())

    def test_closing_a_browser_without_a_job_falls_back_to_killing_the_tree(self):
        proc, profile = self.closed_browser(with_job=False)
        self.assertFalse(fetch.process_alive(proc.pid))
        self.assertIsNotNone(proc.returncode)
        self.assertFalse(profile.exists())

    def run_to_the_end(self):
        proc, job = fetch.spawn_in_job([sys.executable, "-c", SAY_HI], stdout=subprocess.PIPE, text=True)
        self.addCleanup(proc.stdout.close)
        self.addCleanup(lambda: kill_pids([proc.pid]))
        if job:
            self.addCleanup(job.close)
        try:
            output, _ = proc.communicate(timeout=10)
        except subprocess.TimeoutExpired:
            self.fail("tiến trình bị kẹt ở trạng thái tạm dừng")
        return output.strip(), job

    def test_process_still_runs_when_no_job_can_be_made(self):
        with mock.patch.object(fetch.KillJob, "create", return_value=None):
            output, job = self.run_to_the_end()
        self.assertEqual((output, job), ("hi", None))

    def test_process_still_runs_when_it_cannot_join_the_job(self):
        with mock.patch.object(fetch.KillJob, "add", return_value=False):
            output, job = self.run_to_the_end()
        self.assertEqual((output, job), ("hi", None))


class FakeDouyin:
    """Douyin giả: trang video cấp cookie khách, API chi tiết trỏ tới file mp4 trên cùng máy chủ."""

    def __init__(self, media):
        self.site = Site()
        self.site.routes[f"/video/{AWEME}"] = self.page
        self.site.routes["/detail/"] = self.detail
        self.site.routes["/media/ok.mp4"] = media if callable(media) else (lambda handler: send(handler, media))

    def page(self, handler):
        send(
            handler,
            b"<html><body>douyin gia</body></html>",
            headers=[
                ("Content-Type", "text/html; charset=utf-8"),
                ("Set-Cookie", "uifid_temp=fakeuifid; Path=/"),
                ("Set-Cookie", "s_v_web_id=verify_fake; Path=/"),
            ],
        )

    def detail(self, handler):
        body = {
            "aweme_detail": {
                "desc": TITLE,
                "video": {"bit_rate": [{"height": 1080, "play_addr": {"url_list": [self.site.base + "/media/ok.mp4"]}}]},
            }
        }
        send(handler, json.dumps(body).encode(), headers=[("Content-Type", "application/json")])

    def __enter__(self):
        self.site.__enter__()
        return self

    def __exit__(self, *exc):
        self.site.__exit__(*exc)

    @property
    def video_link(self) -> str:
        return f"{self.site.base}/video/{AWEME}"


@unittest.skipUnless(HAS_EDGE, "cần Microsoft Edge trên Windows")
class RealEdge(Scratch):
    """Chạy download() với Edge headless thật. Thư mục tạm của Python trỏ vào thư mục riêng của test."""

    def setUp(self):
        super().setUp()
        self.out = self.root / "tai-ve"
        self.out.mkdir()
        self.temp = self.root / "temp"
        self.temp.mkdir()
        self.full = make_mp4()
        # Dù test pass hay fail, không để Edge của test ở lại máy người dùng.
        self.addCleanup(lambda: kill_pids(edge_pids_using(self.temp)))

    def against(self, fake, action):
        """Chạy `action` với fetch.py trỏ vào Douyin giả và thư mục tạm riêng của test."""
        env = {"TEMP": str(self.temp), "TMP": str(self.temp)}
        with mock.patch.dict(os.environ, env), \
                mock.patch.object(fetch, "PAGE", fake.site.base + "/"), \
                mock.patch.object(fetch, "DETAIL_URL", fake.site.base + "/detail/"):
            return action()

    def run_download(self, fake):
        return self.against(fake, lambda: fetch.download(fake.video_link, str(self.out)))

    def nothing_left_behind(self):
        self.assertEqual(self.profiles_left(), [], "thư mục hồ sơ Edge còn sót")
        self.assertTrue(wait_until(lambda: not edge_pids_using(self.temp), 8, step=1), "Edge của lần tải còn chạy")

    def profiles_left(self):
        return sorted(path.name for path in self.temp.glob("dy-edge-*"))

    def test_whole_download_leaves_no_profile_folder_and_no_edge(self):
        with FakeDouyin(self.full) as fake:
            with contextlib.redirect_stderr(io.StringIO()):
                result = self.run_download(fake)
        self.assertEqual(pathlib.Path(result).read_bytes(), self.full)
        self.nothing_left_behind()

    def test_a_session_that_fails_to_start_leaves_no_profile_and_no_edge(self):
        with FakeDouyin(self.full) as fake:
            with mock.patch.object(fetch, "wait_session", side_effect=RuntimeError("thử")):
                with self.assertRaises(RuntimeError):
                    self.against(fake, lambda: fetch.session(fake.video_link))
        self.nothing_left_behind()

    def test_a_finished_file_is_reused_and_still_leaves_nothing_behind(self):
        name = f"{TITLE} [{AWEME}].mp4"
        (self.out / name).write_bytes(self.full)
        asked = []

        def media(handler):
            asked.append(handler.path)
            send(handler, self.full)

        with FakeDouyin(media) as fake:
            with contextlib.redirect_stderr(io.StringIO()):
                result = self.run_download(fake)
        self.assertEqual(pathlib.Path(result).name, name)
        self.assertEqual(asked, [], "tải lại dù đã có file đủ")
        self.nothing_left_behind()

    def test_a_download_clears_profiles_left_by_dead_runs(self):
        # 999999 không chia hết cho 4 nên không thể là số tiến trình Windows nào.
        stale = self.temp / "dy-edge-999999"
        (stale / "Default").mkdir(parents=True)
        (stale / "Default" / "Cookies").write_bytes(b"x" * 1024)
        with FakeDouyin(self.full) as fake:
            with contextlib.redirect_stderr(io.StringIO()):
                self.run_download(fake)
        self.assertFalse(stale.exists(), "thư mục của lần chạy đã chết còn sót")

    def test_cut_file_of_an_earlier_run_is_replaced_and_its_stale_part_dropped(self):
        name = f"{TITLE} [{AWEME}].mp4"
        (self.out / name).write_bytes(self.full[:150_000])
        (self.out / f"Tên cũ [{AWEME}].mp4.part").write_bytes(b"x" * 100)
        with FakeDouyin(self.full) as fake:
            with contextlib.redirect_stderr(io.StringIO()):
                result = self.run_download(fake)
        self.assertEqual(pathlib.Path(result).read_bytes(), self.full)
        self.assertEqual(sorted(path.name for path in self.out.iterdir()), [name])

    def test_hard_kill_in_the_middle_leaves_no_edge_no_final_file_and_the_profile_is_swept_next_time(self):
        full = make_mp4(1_000_000)
        paused_at = 600_000
        reached, release = threading.Event(), threading.Event()

        def stalled(handler):
            handler.send_response(200)
            handler.send_header("Content-Length", str(len(full)))
            handler.end_headers()
            handler.wfile.write(full[:paused_at])
            handler.wfile.flush()
            reached.set()
            release.wait(60)

        driver = (
            "import importlib.util, sys\n"
            "path, base, link, out = sys.argv[1:5]\n"
            "spec = importlib.util.spec_from_file_location('douyin_fetch', path)\n"
            "fetch = importlib.util.module_from_spec(spec)\n"
            "spec.loader.exec_module(fetch)\n"
            "fetch.PAGE = base + '/'\n"
            "fetch.DETAIL_URL = base + '/detail/'\n"
            "print(fetch.download(link, out), flush=True)\n"
        )
        with FakeDouyin(stalled) as fake:
            env = {**os.environ, "TEMP": str(self.temp), "TMP": str(self.temp)}
            proc = subprocess.Popen(
                [sys.executable, "-c", driver, str(FETCH_PATH), fake.site.base, fake.video_link, str(self.out)],
                env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            )
            self.addCleanup(lambda: kill_pids([proc.pid]))
            try:
                self.assertTrue(reached.wait(90), "Edge hoặc bước tải không chạy tới chỗ chặn")
                edge = edge_pids_using(self.temp)
                self.assertTrue(edge, "không thấy Edge nào của lần tải, test không đo được gì")
                proc.kill()  # đúng như Rust làm khi bấm Dừng
                proc.wait()
                alive = lambda: [pid for pid in edge if fetch.process_alive(pid)]
                self.assertTrue(wait_until(lambda: not alive(), 10), f"Edge mồ côi còn chạy: {alive()}")
            finally:
                release.set()
        self.assertEqual([path.name for path in self.out.glob("*.mp4")], [], "tên cuối giữ file dở")
        removed = fetch.sweep_stale_profiles(self.temp)
        self.assertEqual([path.name for path in removed], [f"dy-edge-{proc.pid}"])
        self.assertEqual(self.profiles_left(), [])


if __name__ == "__main__":
    unittest.main()
