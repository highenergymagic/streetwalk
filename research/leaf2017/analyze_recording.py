"""Extract repeatable coarse measurements from the CC0 LEAF cabin preview.

These are uncalibrated digital levels: the recording has no speed, microphone,
or gain metadata. They locate useful listening regions; they are not SPL.
"""

from pathlib import Path
import csv
import subprocess
import numpy as np
from scipy.signal import welch

ROOT = Path(__file__).resolve().parent
SOURCE = ROOT / "sources/nissan-leaf-interior-martian-hq-preview.mp3"
RATE = 11_025
BANDS = [(20, 80), (80, 250), (250, 800), (800, 2500), (2500, 5000)]


def main():
    raw = subprocess.check_output([
        "ffmpeg", "-v", "error", "-i", str(SOURCE), "-f", "f32le",
        "-ac", "2", "-ar", str(RATE), "-",
    ])
    audio = np.frombuffer(raw, dtype="<f4").reshape(-1, 2)
    rows = []
    for start in range(0, len(audio) // RATE - 4, 5):
        sample = audio[start * RATE:(start + 5) * RATE]
        mono = sample.mean(axis=1)
        frequency, power = welch(mono, RATE, nperseg=8192)
        band_power = [float(np.trapezoid(power[(frequency >= lo) & (frequency < hi)], frequency[(frequency >= lo) & (frequency < hi)])) for lo, hi in BANDS]
        total = max(sum(band_power), 1e-15)
        peaks = np.where((power[1:-1] > power[:-2]) & (power[1:-1] > power[2:]))[0] + 1
        peaks = peaks[(frequency[peaks] >= 80) & (frequency[peaks] <= 3500)]
        strongest = sorted(peaks, key=lambda i: power[i], reverse=True)[:4]
        rows.append({
            "start_s": start,
            "rms_dbfs": round(float(20 * np.log10(np.sqrt(np.mean(mono * mono)) + 1e-12)), 2),
            "left_right_correlation": round(float(np.corrcoef(sample.T)[0, 1]), 3),
            **{f"share_{lo}_{hi}_hz": round(value / total, 4) for (lo, hi), value in zip(BANDS, band_power)},
            **{f"dbfs_{lo}_{hi}_hz": round(float(10 * np.log10(value + 1e-15)), 2) for (lo, hi), value in zip(BANDS, band_power)},
            "top_peaks_hz": " ".join(str(round(float(frequency[i]))) for i in strongest),
        })
    path = ROOT / "recording_windows.csv"
    with path.open("w", newline="", encoding="utf-8") as output:
        writer = csv.DictWriter(output, rows[0].keys())
        writer.writeheader()
        writer.writerows(rows)
    print(f"{len(rows)} five-second windows -> {path}")
    print("Median left/right correlation:", round(float(np.median([r["left_right_correlation"] for r in rows])), 3))
    print("Median RMS dBFS:", round(float(np.median([r["rms_dbfs"] for r in rows])), 2))


if __name__ == "__main__":
    main()
