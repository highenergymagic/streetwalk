# Third-party materials

Streetwalk's original source code is MIT licensed. The following bundled
materials retain their own terms; the root `LICENSE` does not replace them.

| Material | Location | Terms and source |
| --- | --- | --- |
| Soundscape cues | `assets/soundscape/*.wav` | MIT, including Microsoft and Soundscape Community copyright notices in [`assets/soundscape/LICENSE.txt`](assets/soundscape/LICENSE.txt). [Upstream project](https://github.com/soundscape-community/soundscape). |
| MIT KEMAR head-response measurements | `assets/hrtf/*.bin`, `assets/hrtf/compact.zip` | MIT Media Laboratory permits reuse with citation to Bill Gardner and Keith Martin. See [`assets/hrtf/README.txt`](assets/hrtf/README.txt) and the [source archive](https://sound.media.mit.edu/resources/KEMAR.html). |
| LEAF interior recording texture | `assets/ev/leaf_cabin_texture_i16.bin` | Derived from [martian's Freesound recording](https://freesound.org/people/martian/sounds/512492/), licensed CC0 1.0. Processing and provenance are in [`assets/ev/README.txt`](assets/ev/README.txt). |
| Rust dependencies | Linked into the built executable, not vendored in this repository | Versions are pinned in `Cargo.lock`; `build.ps1` gathers their license metadata and notices into `THIRD-PARTY-NOTICES.txt` in the portable build. |
| NVDA Controller Client DLL | Optional portable build component, not committed to this repository | LGPL-2.1. The build copies NV Access's license and readme beside the DLL. [Official API documentation](https://github.com/nvaccess/nvda/blob/master/extras/controllerClient/readme.md). |

OpenStreetMap, Google Maps, TomTom, Open-Meteo, Wikimedia and other service
results are fetched at runtime. Their maps, place descriptions and responses
are not part of this source repository. See [SERVICES.md](SERVICES.md) for
attribution and service details.

Downloaded research articles, manuals, original recordings and the separate
Soundscape reference checkout are excluded from this repository.
