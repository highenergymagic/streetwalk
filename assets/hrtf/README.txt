MIT KEMAR head-related impulse responses

Source: https://sound.media.mit.edu/resources/KEMAR.html
Source archive: https://sound.media.mit.edu/resources/KEMAR/compact.zip
Authors: Bill Gardner and Keith Martin, MIT Media Lab, 1994.

The original compact.zip is retained here. kemar_horizontal_i16.bin contains
the 0-degree elevation WAV responses from that archive, azimuths 0 through
180 degrees at 5-degree steps. Each response is 128 frames of stereo 16-bit
PCM at 44.1 kHz, concatenated without WAV headers. Negative azimuths use
left/right reflection symmetry in the renderer.

kemar_cabin_reflections_i16.bin contains two further 128-frame stereo
responses from the same archive: elevation -30 degrees / azimuth 0 degrees
for the dashboard direction, then elevation +90 degrees / azimuth 0 degrees
for the roof direction. research/prepare_cabin_hrtf.py reproduces this asset.
Side-glass and rear reflections use measured horizontal responses.

MIT's dataset notice says it is free with no restrictions on use provided
the authors are cited in research or commercial applications. This file and
the accompanying documentation provide that citation. These free-field
measurements are not measurements of the interior of a particular vehicle.
