import importlib.util
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import unittest
import wave

from faster_whisper import (
    adjust_end,
    cues_document,
    format_cue,
    format_progress,
    model_kwargs,
    reject_model,
    transcribe_kwargs,
    write_cues,
)


class FasterWhisperFormat(unittest.TestCase):
    def temp(self):
        handle = tempfile.NamedTemporaryFile(delete=False)
        handle.close()
        os.remove(handle.name)
        return handle.name

    def empty_dir(self):
        return tempfile.mkdtemp()

    def test_cue_line_matches_the_watch_parser(self):
        self.assertEqual(
            format_cue(1, 4, " Xin chào "),
            "[00:00:01.000 --> 00:00:04.000]  Xin chào",
        )

    def test_short_end_is_raised_before_the_line(self):
        self.assertEqual(adjust_end(1.0, 1.0), 1.05)

    def test_progress_rounds_and_clamps(self):
        self.assertEqual(format_progress(0.126), "progress = 13%\n")
        self.assertEqual(format_progress(2.0), "progress = 100%\n")

    def test_empty_rows_do_not_create_a_transcript(self):
        path = self.temp()
        self.assertFalse(write_cues(path, [(0.0, 1.0, "  ")]))
        self.assertFalse(os.path.exists(path))

    def test_rows_become_the_saved_cue_shape(self):
        doc = cues_document([(1.0, 1.0, " Xin chào ")])
        self.assertEqual(doc, [{"start": 1.0, "end": 1.05, "text": "Xin chào"}])

    def test_missing_model_bin_is_rejected_before_load(self):
        self.assertEqual(
            reject_model(self.empty_dir()),
            "Chưa thấy model large-v3-turbo trong folder whisper.",
        )

    def test_gpu_settings_stay_fixed(self):
        self.assertEqual(model_kwargs(), {"device": "cuda", "compute_type": "float16"})
        self.assertEqual(
            transcribe_kwargs(),
            {
                "language": None,
                "beam_size": 5,
                "vad_filter": True,
                "vad_parameters": {"min_silence_duration_ms": 500, "speech_pad_ms": 200},
                "condition_on_previous_text": False,
                "word_timestamps": False,
            },
        )

    def test_script_does_not_name_a_downloadable_model(self):
        source = pathlib.Path(__file__).with_name("faster_whisper.py").read_text(encoding="utf-8")
        self.assertNotIn("download_root", source)
        self.assertNotIn("huggingface", source.lower())
        self.assertIn("Không nghe được lời thoại.", source)
        self.assertIn("faster-whisper không chạy được trên GPU.", source)


SCRIPT = pathlib.Path(__file__).with_name("faster_whisper.py")

STUB_LIBRARY = '''import os


class _Segment:
    def __init__(self, start, end, text):
        self.start = start
        self.end = end
        self.text = text


class WhisperModel:
    def __init__(self, model_dir, **kwargs):
        if os.environ.get("STUB_LOAD_ERROR"):
            raise RuntimeError(os.environ["STUB_LOAD_ERROR"])

    def transcribe(self, samples, **kwargs):
        segments = [_Segment(0.0, 2.0, "阿飘 起床啦"), _Segment(2.0, 4.0, "Xin chào đồng bào")]
        return iter(segments), None
'''


# Rust gọi script qua pipe và đọc UTF-8. Python trên Windows ghi pipe theo bảng mã ANSI (cp1252),
# nên test ép cp1252 để lỗi hiện đủ trên mọi máy, không cần GPU.
@unittest.skipUnless(importlib.util.find_spec("numpy"), "cần numpy để đọc audio.wav")
class FasterWhisperProcess(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.folder = pathlib.Path(folder.name)
        library = self.folder / "stub" / "faster_whisper"
        library.mkdir(parents=True)
        (library / "__init__.py").write_text(STUB_LIBRARY, encoding="utf-8")
        self.model = self.folder / "model"
        self.model.mkdir()
        (self.model / "model.bin").write_bytes(b"")
        self.audio = self.folder / "audio.wav"
        with wave.open(str(self.audio), "wb") as handle:
            handle.setnchannels(1)
            handle.setsampwidth(2)
            handle.setframerate(16000)
            handle.writeframes(b"\x00\x00" * 16000)
        self.out = self.folder / "transcript.json"

    def run_script(self, model=None, **extra_env):
        env = dict(os.environ)
        env.update(
            PYTHONPATH=str(self.folder / "stub"),
            PYTHONIOENCODING="cp1252",
            PYTHONUTF8="0",
            **extra_env,
        )
        return subprocess.run(
            [
                sys.executable,
                "-u",
                str(SCRIPT),
                "--model",
                str(model or self.model),
                "--audio",
                str(self.audio),
                "--out",
                str(self.out),
                "--duration",
                "4",
            ],
            env=env,
            stdin=subprocess.DEVNULL,
            capture_output=True,
            timeout=60,
        )

    def test_cues_stay_utf8_when_the_pipe_defaults_to_cp1252(self):
        result = self.run_script()
        self.assertEqual(result.returncode, 0, result.stderr.decode("utf-8", "replace"))
        self.assertEqual(
            result.stdout.decode("utf-8").splitlines(),
            [
                "[00:00:00.000 --> 00:00:02.000]  阿飘 起床啦",
                "[00:00:02.000 --> 00:00:04.000]  Xin chào đồng bào",
            ],
        )
        saved = json.loads(self.out.read_text(encoding="utf-8"))
        self.assertEqual(saved[0]["text"], "阿飘 起床啦")

    def test_messages_stay_readable_when_the_pipe_defaults_to_cp1252(self):
        empty = self.folder / "empty"
        empty.mkdir()
        result = self.run_script(model=empty)
        self.assertEqual(result.returncode, 1)
        self.assertEqual(
            result.stderr.decode("utf-8", "replace").strip(),
            "Chưa thấy model large-v3-turbo trong folder whisper.",
        )

    def test_failure_names_the_real_cause(self):
        result = self.run_script(STUB_LOAD_ERROR="Library cudnn_ops64_9.dll is not found")
        self.assertEqual(result.returncode, 1)
        message = result.stderr.decode("utf-8", "replace")
        self.assertIn("RuntimeError: Library cudnn_ops64_9.dll is not found", message)
        self.assertIn("faster-whisper không chạy được trên GPU.", message)


if __name__ == "__main__":
    unittest.main()
