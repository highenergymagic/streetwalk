"""Make a transformed cabin texture from the CC0 BMW i3 Freesound preview.

Input is development-only: target/reference-ev/bmw-i3-cc0.mp3
Output is a compact, filtered M/S-to-stereo texture used by the grain engine.
Requires ffmpeg, numpy, and scipy. The original recording is never packaged.
"""
from pathlib import Path
import subprocess
import numpy as np
from scipy.signal import butter, sosfiltfilt

ROOT = Path(__file__).resolve().parents[1]
source = ROOT / "target/reference-ev/bmw-i3-cc0.mp3"
destination = ROOT / "assets/ev/i3_cabin_texture_i16.bin"
rate = 44_100
start, duration = 340, 45
command = ["ffmpeg", "-v", "error", "-ss", str(start), "-t", str(duration),
           "-i", str(source), "-f", "f32le", "-ac", "2", "-ar", str(rate), "-"]
raw = subprocess.check_output(command)
ms = np.frombuffer(raw, dtype="<f4").reshape(-1, 2).copy()
# The source explicitly labels its channels as mid and side, not L and R.
stereo = np.stack((ms[:, 0] + 0.65 * ms[:, 1],
                   ms[:, 0] - 0.65 * ms[:, 1]), axis=1)
# The preview contains substantial sub-bass and a fixed vehicle-specific hum.
# Keep the non-periodic tire/cabin texture; the live model supplies body modes.
stereo = sosfiltfilt(butter(3, [115, 3500], btype="bandpass", fs=rate,
                            output="sos"), stereo, axis=0)
stereo = np.clip(stereo * (0.08 / np.sqrt(np.mean(stereo ** 2))), -0.95, 0.95)
destination.parent.mkdir(parents=True, exist_ok=True)
destination.write_bytes((stereo * 32767).astype("<i2").tobytes())
print(f"{destination}: {len(stereo) / rate:.1f}s, {destination.stat().st_size} bytes")
