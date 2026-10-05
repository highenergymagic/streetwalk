"""Extract measured roof and dashboard directions from MIT KEMAR compact.zip."""
from pathlib import Path
from io import BytesIO
import wave
import zipfile

ROOT = Path(__file__).resolve().parents[1]
archive = ROOT / "assets/hrtf/compact.zip"
out = ROOT / "assets/hrtf/kemar_cabin_reflections_i16.bin"
paths = ("elev-30/H-30e000a.wav", "elev90/H90e000a.wav")
with zipfile.ZipFile(archive) as source, out.open("wb") as destination:
    for name in paths:
        with wave.open(BytesIO(source.read(name))) as audio:
            assert (audio.getnchannels(), audio.getframerate(), audio.getsampwidth(),
                    audio.getnframes()) == (2, 44100, 2, 128)
            destination.write(audio.readframes(128))
print(out, out.stat().st_size)
