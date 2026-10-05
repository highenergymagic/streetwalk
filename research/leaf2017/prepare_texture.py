"""Build the LEAF cabin's irregular layer from the CC0 interior recording.

The 2017-specific speed and motor tones remain synthesized at runtime. This
recording has no speed or model-year metadata, so it is used only for a quiet,
band-limited cabin texture. Requires ffmpeg, numpy and scipy.
"""
from pathlib import Path
import subprocess
import numpy as np
from scipy.signal import butter, sosfiltfilt

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "research/leaf2017/sources/nissan-leaf-interior-martian-hq-preview.mp3"
OUTPUT = ROOT / "assets/ev/leaf_cabin_texture_i16.bin"
RATE = 44_100
START = 210
DURATION = 80

raw = subprocess.check_output([
    "ffmpeg", "-v", "error", "-ss", str(START), "-t", str(DURATION),
    "-i", str(SOURCE), "-f", "f32le", "-ac", "2", "-ar", str(RATE), "-",
])
stereo = np.frombuffer(raw, dtype="<f4").reshape(-1, 2).copy()
# Remove microphone handling and fixed low-frequency recording artifacts.
# The dynamic model supplies its own wheel, suspension and motor components.
stereo = sosfiltfilt(butter(3, [115, 3500], btype="bandpass", fs=RATE,
                            output="sos"), stereo, axis=0)
stereo *= 0.08 / np.sqrt(np.mean(stereo ** 2))
stereo = np.clip(stereo, -0.95, 0.95)
OUTPUT.write_bytes((stereo * 32767).astype("<i2").tobytes())
print(f"{OUTPUT}: {len(stereo) / RATE:.1f}s, {OUTPUT.stat().st_size} bytes")
