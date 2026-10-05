# 2017 Nissan LEAF cabin synthesis research

`fetch_sources.py` downloads primary specifications, audio and acoustics papers
into `sources/`; `sources.json` records URLs, sizes and SHA-256 hashes. The
downloads are local research material and are excluded from the portable
package. `prepare_texture.py` creates the bundled CC0 derived texture.
`analyze_recording.py` writes five-second spectral summaries to
`recording_windows.csv`. Both scripts require Python, NumPy, SciPy and ffmpeg.
[Driving behavior and traffic-sound assumptions](driving-behavior.md) document
the newer acceleration, braking, indicator and estimated car-rate model.

## What is usable

| Source | Useful facts | Limits |
| --- | --- | --- |
| [Nissan first-generation brochure](https://www.nissan-cdn.net/content/dam/Nissan/palestine/brochure/Nissan_Leaf.pdf) | EM57, 80 kW, 254 Nm, 8.1938 overall reduction, 205/55R16 tires, curb mass roughly 1475–1562 kg, 0–100 km/h 11.5 s | Market-specific brochure; tire loaded radius and equipment mass vary |
| [Nissan 2017 specifications](https://usa.nissannews.com/en-US/releases/2017-nissan-leaf-specifications) and [owner manual](https://owners.nissanusa.com/content/techpub/ManualsAndGuides/LEAF/2017/2017-LEAF-owner-manual.pdf) | Confirms 2017 power and torque, tires and warning-sound behavior | No cabin acoustic transfer functions |
| [CC0 LEAF interior recording](https://freesound.org/people/martian/sounds/512492/) | 10:50 stereo drive with varied speeds and pedestrian warning enabled; useful nonperiodic cabin detail | Year, speed trace, microphone, gain and road surfaces unknown; public MP3 preview is lossy. Original WAV requires Freesound login |
| [Ricardo interior motor study](https://www.acoustics.asn.au/conference_proceedings/INTERNOISE2014/papers/p578.pdf) | Driver-ear binaural measurement context and motor/inverter tonal families | Earlier LEAF ratio 7.9377; plots are not calibrated impulse responses or raw audio for a 2017 car |
| [Cambridge LEAF road-noise study](https://api.repository.cam.ac.uk/server/api/core/bitstreams/666fec54-c3a5-4504-b2c2-3dd826e8647a/content) | Cabin microphones and suspension measurements identify strong structure-borne road noise below 500 Hz | Road and trim configuration not specified as 2017; raw signals unavailable |
| [ORNL early LEAF teardown](https://info.ornl.gov/sites/publications/files/Pub46325.pdf) | Drivetrain architecture | Its 7.93 ratio belongs to an older car; earlier Streetwalk builds incorrectly used it for 2017 |
| [Danish EV noise report](https://www.vejdirektoratet.dk/sites/default/files/publications/noise_from_electric_vehicles_0.pdf) | Broad tire-versus-propulsion crossover context | Primarily exterior noise, not a cabin calibration |

The CC0 preview was analyzed in 130 five-second windows. Median left/right
correlation is 0.845 and median digital RMS is -31.35 dBFS. Energy below 80 Hz
dominates many windows, which may reflect microphone handling or recording
conditions rather than transferable cabin acoustics. The runtime texture
therefore high-passes at 115 Hz. The 80-second 210–290 s excerpt keeps stereo
irregularity; speed-dependent motor, wheel and suspension sounds are generated
from the virtual vehicle state. These measurements are **not** sound-pressure
levels or a validated 2017 LEAF frequency response.

The [2018 LEAF exterior measurement archive](https://zenodo.org/records/10610491)
offers audio and measurements but its file endpoint returned HTTP 403 during
this research. Its second-generation exterior data would require separate
validation before use in this first-generation cabin model. NHTSA crash
meshes likewise do not provide acoustic body-panel modes or damping.

## Model gaps

The rotor ratio, nominal radius, mass and motor power envelope now use Nissan
figures. The cabin paths still use generic KEMAR HRTFs and approximate image
sources. Suspension modal frequencies, mount transmissibility, glass losses,
seat occlusion, trim rattles and cabin impulse responses are not publicly
available for a 2017 LEAF. A measured speed and microphone position tied to a
calibrated 2017 cabin recording would be the most useful next dataset. Until
then, detailed resonance amplitudes remain perceptual tuning parameters,
not claims about a specific vehicle.
