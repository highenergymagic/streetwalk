use crate::{
    ev_audio::{EvControls, EvInput, EvSource},
    geo::Point,
};
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, SpatialPlayer};
use std::io::Cursor;
use std::sync::Arc;

// These sounds come from Soundscape Community under the MIT license.
// See assets/soundscape/LICENSE.txt.
const POI_CUE: &[u8] = include_bytes!("../assets/soundscape/sense_poi.wav");
const SAFETY_CUE: &[u8] = include_bytes!("../assets/soundscape/sense_safety.wav");
const MOBILITY_CUE: &[u8] = include_bytes!("../assets/soundscape/sense_mobility.wav");
const BEACON_PING: &[u8] = include_bytes!("../assets/soundscape/beacon_ping.wav");
const LEFT_EAR: [f32; 3] = [-0.2, 0., 0.];
const RIGHT_EAR: [f32; 3] = [0.2, 0., 0.];

pub fn emitter(relative_bearing: f64) -> [f32; 3] {
    let radians = relative_bearing.to_radians();
    [radians.sin() as f32, 0., radians.cos() as f32]
}

pub struct Audio {
    output: Option<MixerDeviceSink>,
    beacon: Option<SpatialPlayer>,
    target: Option<Point>,
    cues: Vec<SpatialPlayer>,
    ev: Option<(Player, Arc<EvControls>)>,
}

impl Audio {
    pub fn new() -> Self {
        Self {
            output: DeviceSinkBuilder::open_default_sink().ok(),
            beacon: None,
            target: None,
            cues: vec![],
            ev: None,
        }
    }

    pub fn available(&self) -> bool {
        self.output.is_some()
    }

    pub fn target(&self) -> Option<Point> {
        self.target
    }

    pub fn stop(&mut self) {
        if let Some(beacon) = self.beacon.take() {
            beacon.stop();
        }
        for cue in self.cues.drain(..) {
            cue.stop();
        }
        self.target = None;
    }

    pub fn ev_update(&mut self, input: EvInput) {
        if self.ev.is_none() {
            let Some(output) = &self.output else { return };
            let controls = Arc::new(EvControls::default());
            let player = Player::connect_new(output.mixer());
            player.set_volume(1.);
            player.append(EvSource::new(controls.clone()));
            self.ev = Some((player, controls));
        }
        if let Some((_, controls)) = &self.ev {
            controls.set(input);
        }
    }
    pub fn ev_stop(&mut self) {
        if let Some((player, _)) = self.ev.take() {
            player.stop();
        }
    }

    pub fn toggle_beacon(&mut self, target: Point, position: Point, heading: f64) -> bool {
        if self.target == Some(target) {
            self.stop();
            return false;
        }
        self.stop();
        let Some(output) = &self.output else {
            return false;
        };
        let sound = match Decoder::new_looped(Cursor::new(BEACON_PING)) {
            Ok(sound) => sound,
            Err(_) => return false,
        };
        let beacon = SpatialPlayer::connect_new(
            output.mixer(),
            emitter(position.bearing(target) - heading),
            LEFT_EAR,
            RIGHT_EAR,
        );
        beacon.set_volume(0.18);
        beacon.append(sound);
        self.target = Some(target);
        self.beacon = Some(beacon);
        true
    }

    pub fn cue(&mut self, target: Point, position: Point, heading: f64) {
        self.play_cue(POI_CUE, position.bearing(target) - heading);
    }

    pub fn safety(&mut self, target: Point, position: Point, heading: f64) {
        self.play_cue(SAFETY_CUE, position.bearing(target) - heading);
    }

    pub fn movement(&mut self) {
        self.play_cue(MOBILITY_CUE, 0.);
    }

    fn play_cue(&mut self, bytes: &'static [u8], relative_bearing: f64) {
        let Some(output) = &self.output else {
            return;
        };
        let Ok(sound) = Decoder::new(Cursor::new(bytes)) else {
            return;
        };
        self.cues.retain(|cue| !cue.empty());
        let cue = SpatialPlayer::connect_new(
            output.mixer(),
            emitter(relative_bearing),
            LEFT_EAR,
            RIGHT_EAR,
        );
        cue.set_volume(0.28);
        cue.append(sound);
        self.cues.push(cue);
    }

    pub fn update(&mut self, position: Point, heading: f64, foreground: bool) {
        if let (Some(target), Some(beacon)) = (self.target, &self.beacon) {
            beacon.set_emitter_position(emitter(position.bearing(target) - heading));
            if foreground {
                beacon.play();
            } else {
                beacon.pause();
            }
        }
        if !foreground {
            for cue in &self.cues {
                cue.stop();
            }
            self.cues.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn virtual_heading_positions_sound() {
        assert!(emitter(90.)[0] > 0.99);
        assert!(emitter(270.)[0] < -0.99);
        assert!(emitter(0.)[2] > 0.99);
        assert!(emitter(180.)[2] < -0.99);
    }

    #[test]
    fn bundled_wavs_decode() {
        assert!(Decoder::new(Cursor::new(POI_CUE)).is_ok());
        assert!(Decoder::new(Cursor::new(SAFETY_CUE)).is_ok());
        assert!(Decoder::new(Cursor::new(MOBILITY_CUE)).is_ok());
        assert!(Decoder::new_looped(Cursor::new(BEACON_PING)).is_ok());
    }
}
