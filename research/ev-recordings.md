# EV cabin reference measurements

This records earlier comparison work. The Tesla, Ioniq and BMW source files were research inputs under ignored `target/reference-ev/`; they are not bundled. The current renderer uses a transformed CC0 LEAF interior texture described in [the 2017 LEAF research notes](leaf2017/README.md). The original creators retain their recordings.

| Video | Use | Caution |
| --- | --- | --- |
| [Tesla Model 3 evening drive](https://www.youtube.com/watch?v=EwYRTIABfOg) | No talking; visible speed display, 0–43 mph samples | Microphone position, automatic gain, compression and road surface are unknown; strong sub-80 Hz microphone/vehicle vibration |
| [Tesla Model 3 binaural test drive](https://www.youtube.com/watch?v=CpcIzZIEXhw) | Independent in-cabin road-noise comparison | Audio is almost mono despite “binaural” title; speed is not annotated in this analysis |
| [Ioniq 5 acceleration and noise test](https://www.youtube.com/watch?v=2GNgJlOctNU) | Visible 0–111 km/h run-up in first minute | Later section contains speech; audio channels are effectively mono and its gain appears processed |
| [BMW i3 interior M/S recording](https://freesound.org/people/tferrino/sounds/405284/) | CC0 cabin texture used in an earlier renderer | Freesound MP3 preview rather than original 24-bit WAV; speed is not annotated; M/S channels require decoding |

The audio was extracted with `python -m yt_dlp -x --audio-format wav URL` after updating yt-dlp to 2026.8.19. The older 2026.3.17 executable returned HTTP 403. Video was used only to identify speed and clean sections. `python research/analyze_ev_reference.py` reproduces the spectral table when the source WAVs are in `target/reference-ev/`.

## Findings before revision

At 30 mph in the no-talking Tesla drive (90–93 s), about **93% of 20 Hz–8 kHz power was below 80 Hz** and another **6% was in 80–250 Hz**. At 43 mph (120–123 s), the split was about **95% below 80 Hz** and **4% in 80–250 Hz**. The previous 90 km/h Streetwalk render had less than **2% below 250 Hz** and more than **90% above 800 Hz**. This stark mismatch explains its thin synthetic character.

The YouTube microphone's very low-frequency response cannot be trusted as calibrated sound pressure. Its 25–50 Hz peaks may include body vibration, windshield coupling, and microphone handling. The appropriate conclusion is that the old renderer was much too bright and lacked structure-borne road/body energy; the raw percentages are **not** an EQ target to copy blindly. Recordings from different roads and microphones also vary greatly at similar displayed speeds, so speed alone cannot determine a convincing cabin sound.

The Ioniq video does not supply a clean binaural reference; the first minute is used only to cross-check how the sound changes during a visible run-up. Its later spoken section is excluded. The separate Tesla test supports a low-frequency-dominated cabin texture but has near-identical left and right audio, so it cannot validate spatial positioning. Measured MIT KEMAR HRTFs remain the spatial reference.

## Earlier physical-model revision

The road is a distance-indexed random surface with several spatial wavelengths. Vehicle movement samples that surface; the front and rear wheels encounter related roughness at a wheelbase delay. Tire compliance and suspension/body resonances turn the road profile into low-frequency structure-borne vibration. An air-borne tire branch is filtered by a closed-cabin transmission response. Apparent airflow and stochastic turbulent pressure drive the windshield and side-window branches, with weather wind direction altering the local pressure. Motor electromagnetic orders are excited by calculated rotor speed and wheel torque, then transmitted through a mount/cabin response. The binaural stage follows these source signals rather than generating the vehicle's sound by itself.

That revision remained a generic vehicle. The later LEAF drivetrain update is documented separately. A calibrated result would require multi-speed in-car binaural recordings, tire/surface labels, weather, and synchronized CAN data from one EV, ideally with measured transfer functions from wheel, motor, and window sources to the driver's ears.

## Earlier BMW granular texture

The user reported that the physically driven renderer still sounded synthesized. The [granular EV sound design study](https://snu.elsevierpure.com/en/publications/design-and-evaluation-of-electric-vehicle-sound-using-granular-sy/) supports using recorded material as a synthesis source. An earlier Streetwalk build used a 45-second transformed excerpt of the CC0 BMW i3 recording to carry the irregular cabin texture. The original remains under ignored `target/reference-ev/`; `research/prepare_ev_texture.py` documents the processing into the former bundled asset. Two overlapping grains drew from different places in that excerpt, while virtual speed and road texture set gain. The physical model still generated changing road impacts, motor load, airflow, and spatial passing cars. That 90 km/h demo had similar left/right levels, with measured energy concentrated in the 80–800 Hz range. This was a technical check, not a validation of the current LEAF sound.

## Closed-window wind and cabin space

Listener feedback identified too much apparent outside wind and too little sense of the sound filling the cabin. The wind path gained two cascaded low-pass stages and lower high-frequency transmission through the closed side glass. The cabin renderer uses short image-source paths for the dashboard, roof, rear seating and side glass. The dashboard and roof paths use actual -30° and +90° elevation responses from the [MIT KEMAR dataset](https://sound.media.mit.edu/resources/KEMAR.html); the others use its horizontal responses. This follows the spatial character of early car-cabin reflections described in [automotive binaural response research](https://www.sciencedirect.com/science/article/abs/pii/S0003682X23003158), but path lengths and absorption remain approximations because no measured cabin impulse responses are available for the sampled BMW i3. The current LEAF model still lacks those cabin measurements.

## Render check after revision

The first revised render emphasized sub-250 Hz power, but user listening exposed a faint, isolated motor tone. Later feedback found the granular mix had lost the motor tone and the road rumble varied too little. That renderer restored a torque-linked eighth-order drive component and added fixed, long-scale road roughness that changed the body rumble as the car advanced. In the 90 km/h demo, roughly **24%** of 20 Hz–8 kHz power lies below 80 Hz, **64%** in 80–250 Hz, and **11%** in 250–800 Hz. In a 25–100 Hz band, the 0.1-second rumble envelope has a coefficient of variation of **0.33** in the steady 90 km/h interval; the screened Tesla interval around 30 mph measured **0.37** by the same method. The motor drive order near 905 Hz stands about **12 dB** above its local spectral background at cruise. These measurements verify variation and tonal presence, not perceptual realism. The Tesla recording's stronger sub-80 Hz energy may partly reflect microphone or mounting vibration; matching it numerically would overfit one recording. Further headphone listening remains necessary.

Tire and wind sources were moved primarily into shared cabin energy, with a smaller independent tire-track component and a slight crosswind ear bias. Passing vehicles stayed directional. The earlier front motor was also directional; the later LEAF update centered it after listener feedback.

## LEAF 2017 target update

The active renderer now uses the 2017 Nissan drivetrain specifications and an 80-second transformed CC0 LEAF cabin recording in place of the BMW i3 texture. See [the research directory](leaf2017/README.md) for downloaded sources, measurements, licenses and unresolved calibration gaps. Earlier BMW notes above document the previous iteration, not the current asset.
