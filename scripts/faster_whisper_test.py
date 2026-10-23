import os
import pathlib
import tempfile
import unittest

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
                "vad_filter": False,
                "word_timestamps": False,
            },
        )

    def test_script_does_not_name_a_downloadable_model(self):
        source = pathlib.Path(__file__).with_name("faster_whisper.py").read_text(encoding="utf-8")
        self.assertNotIn("download_root", source)
        self.assertNotIn("huggingface", source.lower())
        self.assertIn("Không nghe được lời thoại.", source)
        self.assertIn("faster-whisper không chạy được trên GPU.", source)


if __name__ == "__main__":
    unittest.main()
