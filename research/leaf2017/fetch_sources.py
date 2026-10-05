"""Fetch the public LEAF research inputs; record checksums for reproducibility.

The Freesound file is a public high-quality MP3 preview. The original 109 MB
WAV requires a Freesound login and is not fetched by this script.
"""

from concurrent.futures import ThreadPoolExecutor, as_completed
from hashlib import sha256
from pathlib import Path
import json
import urllib.request

ROOT = Path(__file__).resolve().parent
DEST = ROOT / "sources"
URLS = {
    "2017-leaf-owner-manual.pdf": "https://owners.nissanusa.com/content/techpub/ManualsAndGuides/LEAF/2017/2017-LEAF-owner-manual.pdf",
    "nissan-leaf-brochure-2016.pdf": "https://www.nissan-cdn.net/content/dam/Nissan/palestine/brochure/Nissan_Leaf.pdf",
    "nissan-leaf-2017-specifications.html": "https://usa.nissannews.com/en-US/releases/2017-nissan-leaf-specifications",
    "ornl-early-leaf-teardown.pdf": "https://info.ornl.gov/sites/publications/files/Pub46325.pdf",
    "ricardo-leaf-interior-tones-2014.pdf": "https://www.acoustics.asn.au/conference_proceedings/INTERNOISE2014/papers/p578.pdf",
    "cambridge-leaf-road-noise-2021.pdf": "https://api.repository.cam.ac.uk/server/api/core/bitstreams/666fec54-c3a5-4504-b2c2-3dd826e8647a/content",
    "danish-electric-vehicle-noise.pdf": "https://www.vejdirektoratet.dk/sites/default/files/publications/noise_from_electric_vehicles_0.pdf",
    "nissan-leaf-interior-martian-hq-preview.mp3": "https://cdn.freesound.org/previews/512/512492_84709-hq.mp3",
}


def fetch(item):
    name, url = item
    path = DEST / name
    if not path.exists():
        request = urllib.request.Request(url, headers={"User-Agent": "Streetwalk research/1.0"})
        with urllib.request.urlopen(request, timeout=90) as response, path.with_suffix(path.suffix + ".part").open("wb") as output:
            while chunk := response.read(1024 * 1024):
                output.write(chunk)
        path.with_suffix(path.suffix + ".part").replace(path)
    digest = sha256(path.read_bytes()).hexdigest()
    return name, {"url": url, "bytes": path.stat().st_size, "sha256": digest}


def main():
    DEST.mkdir(parents=True, exist_ok=True)
    results = {}
    with ThreadPoolExecutor(max_workers=4) as pool:
        for future in as_completed(pool.submit(fetch, item) for item in URLS.items()):
            name, info = future.result()
            results[name] = info
            print(f"{name}: {info['bytes']:,} bytes")
    (ROOT / "sources.json").write_text(json.dumps(dict(sorted(results.items())), indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
