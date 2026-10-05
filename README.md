# Streetwalk

Streetwalk is a Windows desktop application for exploring maps by keyboard and sound. It supports virtual walking, turn-by-turn virtual driving, nearby places, spatial cues, and NVDA speech. OpenStreetMap is the default map source; an optional Google Maps mode uses a user-provided API key. This is an exploration tool, not a physical navigation aid.

**Using Streetwalk?** Read the [user guide](USERGUIDE.md). F1 in the application opens the same guide in your browser.

## Developer quick start

Requirements: Windows x64, Rust stable, a working MSVC or GNU native linker, and NVDA for speech testing. The application can build without NVDA, but a portable package needs the official x64 `nvdaControllerClient.dll` and its `license.txt` and `readme.md` under `dist/nvda-controller/`.

```powershell
cargo fmt --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
powershell -NoProfile -ExecutionPolicy Bypass -File .\build.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File .\package.ps1
```

`build.ps1` checks formatting, runs release tests, builds the release executable, and assembles `dist/Streetwalk`. `package.ps1` runs the build and creates a timestamped portable ZIP and folder under `dist/`. It requires the NVDA controller files so a published package has working speech and the required notices. Both scripts use `tools/w64devkit` if present. The [services and build notes](SERVICES.md) cover service overrides, data storage, and optional network smoke tests.

Download the controller client from [NV Access's official releases](https://download.nvaccess.org/releases/2026.2/nvda_2026.2_controllerClient.zip) and extract it to `dist/nvda-controller/`. The GitHub workflow downloads and verifies that archive automatically. `dist/`, `data/`, API key files, and `tools/` are ignored by Git.

## Code map

| Area | Files |
| --- | --- |
| Windows UI, keyboard input, application state, background requests | `src/ui.rs`, `src/main.rs` |
| OpenStreetMap data, cache, search, service calls | `src/data.rs`, `src/map.rs` |
| Google Places, Geocoding, and Routes | `src/google.rs` |
| Walking/driving routes and geometry | `src/navigation.rs`, `src/geo.rs` |
| NVDA speech and spatial/cabin audio | `src/speech.rs`, `src/audio.rs`, `src/ev_audio.rs` |
| Windows location | `src/location.rs` |
| Audio source research and provenance | `research/`, `assets/`, [audio model notes](EV-AUDIO-MODEL.md) |

The UI keeps map/route downloads on a worker separate from frequent address and traffic requests. Service responses have size limits. OpenStreetMap neighborhoods, searches, and routes can be cached in `data/`; Google content is not persisted as an offline cache. The network provider matrix and privacy implications are in [SERVICES.md](SERVICES.md).

## Portable releases

[`.github/workflows/portable.yml`](.github/workflows/portable.yml) builds and tests on Windows for pushes to `main`, pull requests, and manual runs. It uploads the portable ZIP as a workflow artifact. Pushing a `v*` tag also creates a GitHub Release containing that ZIP; the tag must match `Cargo.toml`'s version (for example, `v0.2.0`). The workflow fetches an official, checksum-pinned NVDA controller client archive and includes its LGPL license and readme.

The portable ZIP contains the executable, controller DLL, third-party notices, [user guide](USERGUIDE.md), source, `Cargo.lock`, build scripts, and an empty `data/` directory. It contains **no API keys**. For a personal local package only, `package.ps1 -IncludeTomTomKey` includes the configured TomTom key; never use that option for a public release. Google keys are never packaged automatically. Users supply their own keys as described in the [user guide](USERGUIDE.md).

## Licensing and contributions

Original Streetwalk code is [MIT licensed](LICENSE). Bundled sounds, HRTF measurements, the LEAF texture, the optional NVDA controller DLL, and Rust dependencies have [separate notices](THIRD_PARTY.md). Preserve those notices when changing assets or packaging. For audio changes, see [EV-AUDIO-MODEL.md](EV-AUDIO-MODEL.md) and the [LEAF research notes](research/leaf2017/README.md).

Run formatting, tests, and Clippy before proposing a change. Network-dependent smoke tests are ignored by default; run them explicitly only when checking provider integration. UI focus and perceived audio behavior need a live Windows/NVDA check.
