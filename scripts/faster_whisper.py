import argparse
import importlib.util
import json
import pathlib
import sys
import wave

def adjust_end(start, end):
    if end < start + 0.05:
        return start + 0.05
    return end


def clock(seconds):
    millis = int(round(max(seconds, 0) * 1000))
    hours, millis = divmod(millis, 3_600_000)
    minutes, millis = divmod(millis, 60_000)
    secs, millis = divmod(millis, 1000)
    return f"{hours:02d}:{minutes:02d}:{secs:02d}.{millis:03d}"


def format_cue(start, end, text):
    end = adjust_end(start, end)
    return f"[{clock(start)} --> {clock(end)}]  {text.strip()}"


def format_progress(fraction):
    percent = int(round(fraction * 100))
    if percent < 0:
        percent = 0
    if percent > 100:
        percent = 100
    return f"progress = {percent}%\n"


def cues_document(rows):
    document = []
    for start, end, text in rows:
        text = text.strip()
        if not text:
            continue
        document.append({"start": start, "end": adjust_end(start, end), "text": text})
    return document


def write_cues(path, rows):
    document = cues_document(rows)
    if not document:
        return False
    pathlib.Path(path).write_text(json.dumps(document, ensure_ascii=False), encoding="utf-8")
    return True


def model_kwargs():
    return {"device": "cuda", "compute_type": "float16"}


def transcribe_kwargs():
    return {
        "language": None,
        "beam_size": 5,
        "vad_filter": True,
        "vad_parameters": {"min_silence_duration_ms": 500, "speech_pad_ms": 200},
        "condition_on_previous_text": False,
        "word_timestamps": False,
    }


def reject_model(model_dir):
    if (pathlib.Path(model_dir) / "model.bin").is_file():
        return None
    return "Chưa thấy model large-v3-turbo trong folder whisper."


def read_wav(path):
    import numpy as np

    with wave.open(path, "rb") as handle:
        if handle.getnchannels() != 1 or handle.getsampwidth() != 2 or handle.getframerate() != 16000:
            raise ValueError("audio.wav phải là PCM 16 kHz mono")
        frames = handle.readframes(handle.getnframes())
    return np.frombuffer(frames, dtype=np.int16).astype(np.float32) / 32768.0


def enable_cuda_dlls():
    import os

    bins = []
    for name in ("nvidia.cublas", "nvidia.cudnn", "nvidia.cuda_nvrtc"):
        spec = importlib.util.find_spec(name)
        if spec is None or not spec.submodule_search_locations:
            continue
        folder = pathlib.Path(list(spec.submodule_search_locations)[0]) / "bin"
        if folder.is_dir():
            os.add_dll_directory(str(folder))
            bins.append(str(folder))
    if bins:
        os.environ["PATH"] = os.pathsep.join(bins) + os.pathsep + os.environ.get("PATH", "")


def installed_library():
    this_dir = pathlib.Path(__file__).resolve().parent
    kept = []
    for entry in sys.path:
        if not entry:
            continue
        try:
            if pathlib.Path(entry).resolve() == this_dir:
                continue
        except OSError:
            continue
        kept.append(entry)
    sys.path[:] = kept
    loaded = sys.modules.get("faster_whisper")
    if loaded is not None and pathlib.Path(getattr(loaded, "__file__", "")).name == "faster_whisper.py":
        del sys.modules["faster_whisper"]
    import faster_whisper

    return faster_whisper


def run(model_dir, audio, out, duration):
    reason = reject_model(model_dir)
    if reason is not None:
        print(reason, file=sys.stderr)
        return 1
    try:
        samples = read_wav(audio)
        enable_cuda_dlls()
        library = installed_library()
        model = library.WhisperModel(str(model_dir), **model_kwargs())
        segments, _info = model.transcribe(samples, **transcribe_kwargs())
        rows = []
        for segment in segments:
            text = segment.text.strip()
            if not text:
                continue
            start = float(segment.start)
            end = adjust_end(start, float(segment.end))
            rows.append((start, end, text))
            print(format_cue(start, end, text), flush=True)
            if duration is not None and duration > 0:
                print(format_progress(end / duration), file=sys.stderr, end="", flush=True)
        if not write_cues(out, rows):
            print("Không nghe được lời thoại.", file=sys.stderr)
            return 1
        if duration is not None and duration > 0:
            print(format_progress(1), file=sys.stderr, end="", flush=True)
        return 0
    except Exception as error:
        reason = f"{type(error).__name__}: {error}"
        print(f"faster-whisper không chạy được trên GPU. {reason}", file=sys.stderr)
        return 1


# Rust đọc stdout và stderr theo UTF-8. Python trên Windows ghi pipe theo bảng mã ANSI (cp1252):
# chữ Trung, đ, ư làm print() báo UnicodeEncodeError, chữ có dấu trong thông báo thành \uXXXX.
def use_utf8_streams():
    for stream in (sys.stdout, sys.stderr):
        if hasattr(stream, "reconfigure"):
            stream.reconfigure(encoding="utf-8", errors="backslashreplace")


def main(argv):
    use_utf8_streams()
    parser = argparse.ArgumentParser()
    parser.add_argument("--model", required=True)
    parser.add_argument("--audio", required=True)
    parser.add_argument("--out", required=True)
    parser.add_argument("--duration", type=float)
    args = parser.parse_args(argv)
    return run(args.model, args.audio, args.out, args.duration)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
