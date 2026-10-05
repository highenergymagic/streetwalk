"""Compare short, manually screened EV cabin intervals with a generated demo.

Development-only dependencies: numpy and scipy. Raw YouTube recordings stay in
target/reference-ev and are not distributed with Streetwalk.
"""
from pathlib import Path
import numpy as np
from scipy.io import wavfile
from scipy.signal import welch

ROOT = Path(__file__).resolve().parents[1]
BANDS = [(20, 80), (80, 250), (250, 800), (800, 2500), (2500, 8000)]
WINDOWS = {
    "EwYRTIABfOg": [(20, "0 mph"), (40, "15 mph"), (50, "25 mph"),
                      (90, "30 mph"), (110, "36 mph"), (120, "43 mph"),
                      (130, "11 mph")],
    "2GNgJlOctNU": [(10, "40 km/h"), (20, "0 km/h"),
                      (30, "107 km/h"), (40, "111 km/h"),
                      (50, "69 km/h")],
    "ev-cabin-demo": [(1, "accelerating"), (3, "accelerating"),
                       (6, "90 km/h"), (9, "braking")],
}


def summarize(path: Path, windows: list[tuple[int, str]]) -> None:
    rate, data = wavfile.read(path, mmap=True)
    print(f"\n{path.name} ({rate} Hz, {data.shape[1]} channels)")
    for second, label in windows:
        channels = data[second * rate:(second + 3) * rate].astype(np.float32) / 32768
        if len(channels) < rate:
            continue
        mono = channels.mean(axis=1)
        freq, power = welch(mono, rate, nperseg=16384)
        energy = np.array([
            np.trapezoid(power[(freq >= low) & (freq < high)],
                         freq[(freq >= low) & (freq < high)])
            for low, high in BANDS
        ])
        shares = energy / max(energy.sum(), 1e-20)
        correlation = np.corrcoef(channels.T)[0, 1]
        dbfs = 20 * np.log10(np.sqrt(np.mean(mono**2)) + 1e-12)
        print(f"{second:>3}s {label:>14}: {dbfs:5.1f} dBFS; "
              f"band shares {shares.round(3).tolist()}; "
              f"L/R corr {correlation:.2f}")


if __name__ == "__main__":
    for name, windows in WINDOWS.items():
        path = (ROOT / "target" / "ev-cabin-demo.wav" if name == "ev-cabin-demo"
                else ROOT / "target" / "reference-ev" / f"{name}.wav")
        if path.exists():
            summarize(path, windows)
