//! Binaural, headphone-oriented synthesis of an electric vehicle cabin.
//! KEMAR ear impulse responses are measured data; the vehicle and cabin are
//! parameterized approximations, not measurements of a particular car.
use rodio::Source;
use std::{
    f32::consts::TAU,
    num::NonZero,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, OnceLock,
    },
    time::Duration,
};

const RATE: f32 = 44_100.;
const TAPS: usize = 128;
const DIRECTIONS: usize = 37;
const HRIR_BYTES: &[u8] = include_bytes!("../assets/hrtf/kemar_horizontal_i16.bin");
const CABIN_HRIR_BYTES: &[u8] = include_bytes!("../assets/hrtf/kemar_cabin_reflections_i16.bin");
const CABIN_TEXTURE: &[u8] = include_bytes!("../assets/ev/leaf_cabin_texture_i16.bin");
const CABIN_TEXTURE_FRAMES: usize = CABIN_TEXTURE.len() / 4;

#[derive(Clone, Copy, Default)]
pub struct EvInput {
    pub speed_kmh: f32,
    pub acceleration_mps2: f32,
    pub grade: f32,
    pub road_texture: f32,
    pub bumpiness: f32,
    pub volume: f32,
    pub wind_kmh: f32,
    /// Meteorological direction relative to the car: 0 is from ahead, 90 from right.
    pub wind_from_deg: f32,
    pub traffic_density: f32,
    pub precipitation_mm: f32,
    /// 1 = hump/bump, 2 = rumble strip. The token advances on each event.
    pub road_event_kind: u8,
    pub road_event_token: u32,
    /// -1 for left, 1 for right, 0 for off.
    pub indicator: i8,
    pub active: bool,
}

#[derive(Default)]
pub struct EvControls {
    speed: AtomicU32,
    acceleration: AtomicU32,
    grade: AtomicU32,
    texture: AtomicU32,
    bumpiness: AtomicU32,
    volume: AtomicU32,
    wind: AtomicU32,
    wind_from: AtomicU32,
    traffic: AtomicU32,
    precipitation: AtomicU32,
    road_event_kind: AtomicU32,
    road_event_token: AtomicU32,
    indicator: AtomicU32,
    active: AtomicU32,
}
impl EvControls {
    pub fn set(&self, input: EvInput) {
        let store = |slot: &AtomicU32, value: f32| slot.store(value.to_bits(), Ordering::Relaxed);
        store(&self.speed, input.speed_kmh.max(0.));
        store(&self.acceleration, input.acceleration_mps2);
        store(&self.grade, input.grade.clamp(-0.08, 0.08));
        store(&self.texture, input.road_texture);
        store(&self.bumpiness, input.bumpiness.clamp(0., 1.));
        store(&self.volume, input.volume.clamp(0., 1.));
        store(&self.wind, input.wind_kmh.max(0.));
        store(&self.wind_from, input.wind_from_deg);
        store(&self.traffic, input.traffic_density.clamp(0., 1.));
        store(&self.precipitation, input.precipitation_mm.max(0.));
        self.road_event_kind
            .store(input.road_event_kind as u32, Ordering::Relaxed);
        self.road_event_token
            .store(input.road_event_token, Ordering::Release);
        self.indicator.store(
            input.indicator.clamp(-1, 1) as i32 as u32,
            Ordering::Relaxed,
        );
        self.active
            .store(u32::from(input.active), Ordering::Relaxed);
    }
    fn read(slot: &AtomicU32) -> f32 {
        f32::from_bits(slot.load(Ordering::Relaxed))
    }
}

struct Hrtf {
    samples: [[[f32; TAPS]; 2]; DIRECTIONS],
}
impl Hrtf {
    fn cabin_reflection(index: usize) -> [[f32; TAPS]; 2] {
        assert!(index < 2);
        assert_eq!(CABIN_HRIR_BYTES.len(), 2 * 2 * TAPS * 2);
        let mut result = [[0.; TAPS]; 2];
        for tap in 0..TAPS {
            for (ear, impulse) in result.iter_mut().enumerate() {
                let byte = ((index * TAPS + tap) * 2 + ear) * 2;
                impulse[tap] =
                    i16::from_le_bytes([CABIN_HRIR_BYTES[byte], CABIN_HRIR_BYTES[byte + 1]]) as f32
                        / 32768.;
            }
        }
        result
    }
    fn measured() -> &'static Self {
        static DATA: OnceLock<Hrtf> = OnceLock::new();
        DATA.get_or_init(|| {
            assert_eq!(HRIR_BYTES.len(), DIRECTIONS * 2 * TAPS * 2);
            let mut samples = [[[0.; TAPS]; 2]; DIRECTIONS];
            for (azimuth, direction) in samples.iter_mut().enumerate() {
                for tap in 0..TAPS {
                    for (ear, impulse) in direction.iter_mut().enumerate() {
                        let byte = ((azimuth * TAPS + tap) * 2 + ear) * 2;
                        *impulse.get_mut(tap).unwrap() =
                            i16::from_le_bytes([HRIR_BYTES[byte], HRIR_BYTES[byte + 1]]) as f32
                                / 32_768.;
                    }
                }
            }
            Hrtf { samples }
        })
    }
    fn taps(angle_deg: f32) -> [[f32; TAPS]; 2] {
        let angle = angle_deg.clamp(-180., 180.);
        let coordinate = angle.abs() / 5.;
        let lower = (coordinate.floor() as usize).min(DIRECTIONS - 1);
        let upper = (lower + 1).min(DIRECTIONS - 1);
        let blend = coordinate.fract();
        let data = Self::measured();
        let mut result = [[0.; TAPS]; 2];
        for (ear, result_ear) in result.iter_mut().enumerate() {
            let source_ear = if angle < 0. { 1 - ear } else { ear };
            for (tap, output) in result_ear.iter_mut().enumerate() {
                *output = data.samples[lower][source_ear][tap] * (1. - blend)
                    + data.samples[upper][source_ear][tap] * blend;
            }
        }
        result
    }
}

struct Binaural {
    history: [f32; TAPS],
    cursor: usize,
    angle: f32,
    taps: [[f32; TAPS]; 2],
}
impl Binaural {
    fn elevated_reflection(index: usize) -> Self {
        Self {
            history: [0.; TAPS],
            cursor: 0,
            angle: 0.,
            taps: Hrtf::cabin_reflection(index),
        }
    }
    fn new(angle: f32) -> Self {
        Self {
            history: [0.; TAPS],
            cursor: 0,
            angle,
            taps: Hrtf::taps(angle),
        }
    }
    fn set_angle(&mut self, angle: f32) {
        if (self.angle - angle).abs() >= 2.5 {
            self.angle = angle;
            self.taps = Hrtf::taps(angle);
        }
    }
    fn render(&mut self, sample: f32) -> [f32; 2] {
        self.history[self.cursor] = sample;
        let mut output = [0.; 2];
        let mut at = self.cursor;
        for tap in 0..TAPS {
            let past = self.history[at];
            output[0] += past * self.taps[0][tap];
            output[1] += past * self.taps[1][tap];
            at = if at == 0 { TAPS - 1 } else { at - 1 };
        }
        self.cursor = (self.cursor + 1) % TAPS;
        output
    }
}

struct Noise(u32);
impl Noise {
    fn new(seed: u32) -> Self {
        Self(seed)
    }
    fn sample(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 as i32 as f32) / i32::MAX as f32
    }
    fn unit(&mut self) -> f32 {
        (self.sample() + 1.) * 0.5
    }
}

/// Overlapping grains from a transformed in-car recording retain the irregular
/// tire, trim and cabin texture that a few oscillators or noise filters miss.
/// The recording is already inside a cabin, so it bypasses the outdoor HRTF.
#[derive(Clone, Copy)]
struct TextureGrain {
    start: usize,
    elapsed: u32,
    duration: u32,
    rate: f32,
}

struct RecordedCabin {
    rng: Noise,
    grains: [TextureGrain; 2],
}
impl RecordedCabin {
    const MAX_DURATION: u32 = 12 * 44_100;
    fn new() -> Self {
        assert_eq!(CABIN_TEXTURE.len() % 4, 0);
        assert!(CABIN_TEXTURE_FRAMES > Self::MAX_DURATION as usize + 1);
        let mut result = Self {
            rng: Noise::new(0x17c3_a51d),
            grains: [TextureGrain {
                start: 0,
                elapsed: 0,
                duration: Self::MAX_DURATION,
                rate: 1.,
            }; 2],
        };
        result.restart(0, 0);
        result.restart(1, 5 * 44_100);
        result
    }
    fn restart(&mut self, index: usize, elapsed: u32) {
        let margin = (Self::MAX_DURATION as f32 * 1.03) as usize;
        let start = (self.rng.unit() * (CABIN_TEXTURE_FRAMES - margin) as f32) as usize;
        let rate = 0.98 + self.rng.unit() * 0.04;
        let duration = ((8. + self.rng.unit() * 4.) * RATE) as u32;
        self.grains[index] = TextureGrain {
            start,
            elapsed,
            duration,
            rate,
        };
    }
    fn read(frame: usize) -> [f32; 2] {
        let offset = frame * 4;
        [
            i16::from_le_bytes([CABIN_TEXTURE[offset], CABIN_TEXTURE[offset + 1]]) as f32 / 32768.,
            i16::from_le_bytes([CABIN_TEXTURE[offset + 2], CABIN_TEXTURE[offset + 3]]) as f32
                / 32768.,
        ]
    }
    fn render(&mut self, metres_per_second: f32, road_texture: f32) -> [f32; 2] {
        let mut result = [0.; 2];
        let mut weight = 0.;
        for index in 0..2 {
            if self.grains[index].elapsed >= self.grains[index].duration {
                self.restart(index, 0);
            }
            let grain = &mut self.grains[index];
            let position = grain.start as f32 + grain.elapsed as f32 * grain.rate;
            let frame = position as usize;
            let fraction = position.fract();
            let current = Self::read(frame);
            let next = Self::read(frame + 1);
            let phase = grain.elapsed as f32 / grain.duration as f32;
            let envelope = 0.5 - 0.5 * (TAU * phase).cos();
            weight += envelope;
            for ear in 0..2 {
                result[ear] += (current[ear] * (1. - fraction) + next[ear] * fraction) * envelope;
            }
            grain.elapsed += 1;
        }
        for sample in &mut result {
            *sample /= weight.max(0.2);
        }
        let gain = (metres_per_second / 14.).clamp(0., 1.).powf(0.8) * road_texture.clamp(0.5, 1.5);
        [result[0] * gain, result[1] * gain]
    }
}

#[derive(Default)]
struct Color {
    bass: f32,
    mid: f32,
    high: f32,
}
impl Color {
    fn split(&mut self, white: f32) -> (f32, f32, f32) {
        self.bass += 0.018 * (white - self.bass);
        self.mid += 0.16 * (white - self.mid);
        self.high += 0.48 * (white - self.high);
        (self.bass, self.mid - self.bass, self.high - self.mid)
    }
}

/// A stationary road height field. Time variation appears only as wheels move
/// through it; rear wheels encounter the same longitudinal profile later.
struct RoadProfile;
impl RoadProfile {
    fn random(cell: i64, seed: u32) -> f32 {
        let mut n = (cell as u32).wrapping_mul(0x9e37_79b9) ^ seed;
        n ^= n >> 16;
        n = n.wrapping_mul(0x7feb_352d);
        n ^= n >> 15;
        n = n.wrapping_mul(0x846c_a68b);
        n ^= n >> 16;
        (n as i32 as f32) / i32::MAX as f32
    }
    fn wavelength(position: f64, metres: f64, seed: u32) -> f32 {
        let coordinate = position / metres;
        let cell = coordinate.floor() as i64;
        let fraction = (coordinate - cell as f64) as f32;
        let smooth = fraction * fraction * (3. - 2. * fraction);
        Self::random(cell, seed) * (1. - smooth) + Self::random(cell + 1, seed) * smooth
    }
    fn height(position: f64, side: u32) -> f32 {
        // ISO-style multi-scale roughness, with shared long wavelengths across
        // the vehicle and independent shorter wavelengths at each tire track.
        Self::wavelength(position, 3.2, 0x1234_abcd) * 0.20
            + Self::wavelength(position, 0.9, 0x1191_0537) * 0.28
            + Self::wavelength(position, 0.24, side ^ 0x4acd_8f12) * 0.30
            + Self::wavelength(position, 0.055, side ^ 0x7181_27ef) * 0.12
    }
    fn defect(position: f64, side: u32, bumpiness: f32) -> f32 {
        // Deterministic sparse defects in road distance, rather than a timed
        // pulse. Even a smooth urban road retains a small chance of a defect.
        let cell = (position / 7.).floor() as i64;
        let mut height = 0.;
        for index in (cell - 1)..=(cell + 1) {
            let chance = (Self::random(index, side ^ 0x41a8_019d) + 1.) * 0.5;
            if chance > 0.025 + bumpiness * bumpiness * 0.30 {
                continue;
            }
            let center =
                (index as f64 + 0.15 + (Self::random(index, 0x5f18_71cd) as f64 + 1.) * 0.35) * 7.;
            let width = 0.18 + (Self::random(index, 0x1917_97ab).abs() as f64) * 0.40;
            let offset = (position - center) / width;
            height += (-(offset * offset) * 2.).exp() as f32
                * (0.18 + bumpiness * 0.75)
                * if Self::random(index, 0x78a2_b0df) > 0. {
                    1.
                } else {
                    -1.
                };
        }
        height
    }
    fn contact(position: f64, side: u32, bumpiness: f32) -> f32 {
        let surface = |at| Self::height(at, side) + Self::defect(at, side, bumpiness);
        let front = surface(position) - surface(position - 0.10);
        let rear = surface(position - 2.8) - surface(position - 2.9);
        front * 0.8 + rear * 0.35
    }
    fn tread(position: f64, side: u32) -> f32 {
        let sample = |x: f64| {
            Self::wavelength(x, 0.11, side ^ 0x7181_27ef) * 0.22
                + Self::wavelength(x, 0.073, side ^ 0x983a_4521) * 0.22
                + Self::wavelength(x, 0.048, side ^ 0x1d47_ac32) * 0.20
                + Self::wavelength(x, 0.029, side ^ 0x1f29_b048) * 0.20
                + Self::wavelength(x, 0.017, side ^ 0x65d4_ac1e) * 0.16
        };
        sample(position) - sample(position - 0.012)
    }
    fn roughness(position: f64) -> f32 {
        // Long road sections alter cabin rumble over fractions of a second to
        // several seconds, while fine tread texture remains more continuous.
        (1. + Self::wavelength(position, 5., 0x9283_7141) * 0.55
            + Self::wavelength(position, 17., 0x31ab_89c1) * 0.35
            + Self::wavelength(position, 53., 0x62f1_a270) * 0.20)
            .clamp(0.4, 1.8)
    }
}

#[derive(Clone, Copy)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}
impl Biquad {
    fn filter(frequency: f32, q: f32, lowpass: bool) -> Self {
        let omega = TAU * frequency / RATE;
        let alpha = omega.sin() / (2. * q);
        let a0 = 1. + alpha;
        let (b0, b1, b2) = if lowpass {
            let b0 = (1. - omega.cos()) * 0.5;
            (b0, 2. * b0, b0)
        } else {
            (alpha, 0., -alpha)
        };
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: -2. * omega.cos() / a0,
            a2: (1. - alpha) / a0,
            z1: 0.,
            z2: 0.,
        }
    }
    fn bandpass(frequency: f32, q: f32) -> Self {
        Self::filter(frequency, q, false)
    }
    fn lowpass(frequency: f32) -> Self {
        Self::filter(frequency, 0.707, true)
    }
    fn process(&mut self, input: f32) -> f32 {
        let output = self.b0 * input + self.z1;
        self.z1 = self.b1 * input - self.a1 * output + self.z2;
        self.z2 = self.b2 * input - self.a2 * output;
        output
    }
}

struct PassBy {
    side: f32,
    elapsed: f32,
    duration: f32,
    strength: f32,
    phase: f32,
}
impl PassBy {
    fn envelope(progress: f32) -> f32 {
        // A close pass peaks briefly beside the listener, then falls away.
        let distance = (progress - 0.5) / 0.16;
        (-0.5 * distance * distance).exp()
    }
}

struct Cabin {
    left: [f32; 4096],
    right: [f32; 4096],
    excitation: [f32; 4096],
    vents: [f32; 4096],
    cursor: usize,
    dashboard: Binaural,
    roof: Binaural,
    rear: Binaural,
    left_glass: Binaural,
    right_glass: Binaural,
    left_vent: Binaural,
    right_vent: Binaural,
    dashboard_absorption: Biquad,
    roof_absorption: Biquad,
    rear_absorption: Biquad,
    left_absorption: Biquad,
    right_absorption: Biquad,
    vent_absorption: [Biquad; 2],
}
impl Cabin {
    fn new() -> Self {
        Self {
            left: [0.; 4096],
            right: [0.; 4096],
            excitation: [0.; 4096],
            vents: [0.; 4096],
            cursor: 0,
            dashboard: Binaural::elevated_reflection(0),
            roof: Binaural::elevated_reflection(1),
            rear: Binaural::new(160.),
            left_glass: Binaural::new(-95.),
            right_glass: Binaural::new(95.),
            left_vent: Binaural::new(-35.),
            right_vent: Binaural::new(35.),
            dashboard_absorption: Biquad::lowpass(1300.),
            roof_absorption: Biquad::lowpass(1900.),
            rear_absorption: Biquad::lowpass(900.),
            left_absorption: Biquad::lowpass(2600.),
            right_absorption: Biquad::lowpass(2600.),
            vent_absorption: [Biquad::lowpass(1600.), Biquad::lowpass(1500.)],
        }
    }
    fn render(&mut self, direct: [f32; 2], source: f32, vent_source: f32) -> [f32; 2] {
        let delayed = |buffer: &[f32; 4096], n: usize, at: usize| buffer[(at + 4096 - n) % 4096];
        // Image-source paths: front dash, roof liner, rear seats and side glass.
        // Material filters damp the reflected high frequencies before their
        // direction-specific, measured two-ear impulse responses.
        let paths = [
            self.dashboard.render(
                self.dashboard_absorption
                    .process(delayed(&self.excitation, 130, self.cursor))
                    * 0.14,
            ),
            self.roof.render(
                self.roof_absorption
                    .process(delayed(&self.excitation, 210, self.cursor))
                    * 0.11,
            ),
            self.rear.render(
                self.rear_absorption
                    .process(delayed(&self.excitation, 410, self.cursor))
                    * 0.075,
            ),
            self.left_glass.render(
                self.left_absorption
                    .process(delayed(&self.excitation, 165, self.cursor))
                    * 0.06,
            ),
            self.right_glass.render(
                self.right_absorption
                    .process(delayed(&self.excitation, 185, self.cursor))
                    * 0.06,
            ),
        ];
        let mut result = direct;
        for path in paths {
            result[0] += path[0];
            result[1] += path[1];
        }
        // Two dashboard vents provide short, separate binaural paths. Their
        // air signal does not excite every wall as a single omnidirectional wash.
        let vent_paths = [
            self.left_vent.render(
                self.vent_absorption[0].process(delayed(&self.vents, 78, self.cursor)) * 0.16,
            ),
            self.right_vent.render(
                self.vent_absorption[1].process(delayed(&self.vents, 112, self.cursor)) * 0.16,
            ),
        ];
        for path in vent_paths {
            result[0] += path[0];
            result[1] += path[1];
        }
        // A small diffuse tail joins the discrete surfaces without a long,
        // metallic room reverb that would imply a much larger space.
        result[0] += delayed(&self.left, 907, self.cursor) * 0.025
            + delayed(&self.right, 1211, self.cursor) * 0.018;
        result[1] += delayed(&self.right, 941, self.cursor) * 0.025
            + delayed(&self.left, 1277, self.cursor) * 0.018;
        self.left[self.cursor] = direct[0];
        self.right[self.cursor] = direct[1];
        self.excitation[self.cursor] = source;
        self.vents[self.cursor] = vent_source;
        self.cursor = (self.cursor + 1) % 4096;
        result
    }
}

pub struct EvSource {
    controls: Arc<EvControls>,
    speed: f32,
    acceleration: f32,
    grade: f32,
    wind: f32,
    wind_from: f32,
    texture: f32,
    bumpiness: f32,
    volume: f32,
    traffic: f32,
    precipitation: f32,
    gain: f32,
    motor_phase: f32,
    traction_phase: f32,
    regen_phase: f32,
    motor_burst_age: f32,
    motor_event_direction: i8,
    road_distance: f64,
    road_event_token: u32,
    road_event_kind: u8,
    road_event_age: f32,
    road_event_speed: f32,
    road_event_noise: Noise,
    road_event_filter: Biquad,
    wind_noise: Noise,
    traffic_noise: Noise,
    traffic_color: Color,
    passing_glass: [Biquad; 2],
    indicator_noise: Noise,
    indicator_filter: Biquad,
    stalk_noise: Noise,
    stalk_filter: Biquad,
    stalk_age: f32,
    stalk_engaging: bool,
    indicator_side: i8,
    indicator_phase: f32,
    indicator_click_age: f32,
    indicator_second_click: bool,
    indicator_first_tick: bool,
    cabin_air_left: Noise,
    cabin_air_right: Noise,
    cabin_air_filters: [Biquad; 2],
    cabin_low_air: Biquad,
    distant_traffic: Biquad,
    fan_phase: f32,
    wiper_phase: f32,
    rain_drop_age: f32,
    rain_drop_right: bool,
    rain_noise: Noise,
    wiper_filter: Biquad,
    rain_filter: Biquad,
    body_left: [Biquad; 3],
    body_right: [Biquad; 3],
    tire_air_left: Biquad,
    tire_air_right: Biquad,
    wind_pressure: [Biquad; 2],
    wind_air: Biquad,
    closed_window: Biquad,
    motor_mount: Biquad,
    gust_slow: f32,
    gust_fast: f32,
    pass_wait: f32,
    passing: Option<PassBy>,
    other_car: Binaural,
    cabin: Cabin,
    recorded_cabin: RecordedCabin,
    right_sample: f32,
    next_is_right: bool,
    frames: u64,
}
impl EvSource {
    fn motor_burst_gain(age: f32) -> f32 {
        // The centered path replaces two old paths. Keep a clear launch level,
        // then taper linearly so it remains audible through the short burst.
        1.2 * (1. - age / 1.5).clamp(0., 1.)
    }

    fn motor_presence(speed_kmh: f32, acceleration_mps2: f32) -> f32 {
        let launch = (1. - speed_kmh / 60.).clamp(0., 1.).powi(2);
        let demand = (acceleration_mps2 / 1.5).clamp(0., 1.);
        1.6 + 5.0 * launch * demand
    }
    pub fn new(controls: Arc<EvControls>) -> Self {
        Self {
            controls,
            speed: 0.,
            acceleration: 0.,
            grade: 0.,
            wind: 0.,
            wind_from: 0.,
            texture: 1.,
            bumpiness: 0.2,
            volume: 0.,
            traffic: 0.,
            precipitation: 0.,
            gain: 0.,
            motor_phase: 0.,
            traction_phase: 0.,
            regen_phase: 0.,
            motor_burst_age: 1.5,
            motor_event_direction: 0,
            road_distance: 0.,
            road_event_token: 0,
            road_event_kind: 0,
            road_event_age: 1.,
            road_event_speed: 0.,
            road_event_noise: Noise::new(0x8ff1_64c2),
            road_event_filter: Biquad::lowpass(280.),
            wind_noise: Noise::new(0x37de_1a91),
            traffic_noise: Noise::new(0xa173_8d11),
            traffic_color: Color::default(),
            passing_glass: [Biquad::lowpass(650.), Biquad::lowpass(380.)],
            indicator_noise: Noise::new(0x1ad1_c470),
            indicator_filter: Biquad::bandpass(1_100., 0.8),
            stalk_noise: Noise::new(0x6c19_3e27),
            stalk_filter: Biquad::lowpass(720.),
            stalk_age: 1.,
            stalk_engaging: false,
            indicator_side: 0,
            indicator_phase: 0.,
            indicator_click_age: 1.,
            indicator_second_click: false,
            indicator_first_tick: false,
            cabin_air_left: Noise::new(0x6d4b_1107),
            cabin_air_right: Noise::new(0x3e21_98ba),
            cabin_air_filters: [Biquad::bandpass(680., 0.75), Biquad::bandpass(590., 0.8)],
            cabin_low_air: Biquad::lowpass(105.),
            distant_traffic: Biquad::lowpass(230.),
            fan_phase: 0.,
            wiper_phase: 0.,
            rain_drop_age: 1.,
            rain_drop_right: false,
            rain_noise: Noise::new(0x6410_ef93),
            wiper_filter: Biquad::bandpass(850., 0.7),
            rain_filter: Biquad::lowpass(1800.),
            body_left: [
                Biquad::bandpass(42., 2.1),
                Biquad::bandpass(83., 2.4),
                Biquad::bandpass(152., 2.0),
            ],
            body_right: [
                Biquad::bandpass(39., 2.0),
                Biquad::bandpass(91., 2.2),
                Biquad::bandpass(164., 1.8),
            ],
            tire_air_left: Biquad::lowpass(1500.),
            tire_air_right: Biquad::lowpass(1400.),
            wind_pressure: [Biquad::bandpass(55., 1.4), Biquad::bandpass(118., 1.2)],
            wind_air: Biquad::lowpass(650.),
            closed_window: Biquad::lowpass(420.),
            motor_mount: Biquad::lowpass(1200.),
            gust_slow: 0.,
            gust_fast: 0.,
            pass_wait: 1.,
            passing: None,
            other_car: Binaural::new(-30.),
            cabin: Cabin::new(),
            recorded_cabin: RecordedCabin::new(),
            right_sample: 0.,
            next_is_right: false,
            frames: 0,
        }
    }
    fn smooth(old: &mut f32, target: f32, seconds: f32) {
        *old += (target - *old) / (RATE * seconds);
    }
    fn render_frame(&mut self) -> [f32; 2] {
        Self::smooth(
            &mut self.speed,
            EvControls::read(&self.controls.speed),
            0.08,
        );
        Self::smooth(
            &mut self.acceleration,
            EvControls::read(&self.controls.acceleration),
            0.12,
        );
        Self::smooth(&mut self.grade, EvControls::read(&self.controls.grade), 0.6);
        let motor_direction = if self.acceleration > 0.35 {
            1
        } else if self.acceleration < -0.35 {
            -1
        } else {
            0
        };
        if motor_direction != 0 && motor_direction != self.motor_event_direction {
            self.motor_burst_age = 0.;
        }
        self.motor_event_direction = motor_direction;
        self.motor_burst_age = (self.motor_burst_age + 1. / RATE).min(1.5);
        Self::smooth(&mut self.wind, EvControls::read(&self.controls.wind), 1.5);
        Self::smooth(
            &mut self.wind_from,
            EvControls::read(&self.controls.wind_from),
            1.5,
        );
        Self::smooth(
            &mut self.texture,
            EvControls::read(&self.controls.texture).max(0.5),
            0.5,
        );
        Self::smooth(
            &mut self.bumpiness,
            EvControls::read(&self.controls.bumpiness),
            1.,
        );
        Self::smooth(
            &mut self.volume,
            EvControls::read(&self.controls.volume),
            0.08,
        );
        Self::smooth(
            &mut self.traffic,
            EvControls::read(&self.controls.traffic),
            2.,
        );
        Self::smooth(
            &mut self.precipitation,
            EvControls::read(&self.controls.precipitation),
            2.,
        );
        Self::smooth(
            &mut self.gain,
            self.controls.active.load(Ordering::Relaxed) as f32,
            0.08,
        );

        let metres_per_second = self.speed.max(0.) / 3.6;
        // Wheel force pays for acceleration, rolling resistance and drag. The
        // reduction gear then converts wheel force to rotor shaft torque.
        let drag = 0.5 * 1.225 * 0.65 * metres_per_second.powi(2);
        // 2017 LEAF: 80 kW EM57, 254 Nm, 8.1938 reduction, 205/55R16 tires.
        // Radius is the nominal tire radius; loaded radius and body mass vary.
        let rolling = 1520. * 9.81 * 0.010;
        let shaft_torque =
            (1520. * (self.acceleration + 9.81 * self.grade) + drag + rolling) * 0.316 / 8.1938;
        let wheel_hz = metres_per_second / (TAU * 0.316);
        let rotor_hz = wheel_hz * 8.1938;
        let rotor_radians_per_second = rotor_hz * TAU;
        let available_torque = (80_000. / rotor_radians_per_second.max(1.)).min(254.);
        let torque = (shaft_torque / available_torque).clamp(-1., 1.);
        let order_24_hz = rotor_hz * 24.;
        self.motor_phase = (self.motor_phase + order_24_hz / RATE).fract();
        self.traction_phase = (self.traction_phase + rotor_hz * 8. / RATE).fract();
        self.regen_phase = (self.regen_phase + rotor_hz * 12. / RATE).fract();
        let motor_angle = self.motor_phase * TAU;
        // A low drive order restores audible traction feedback. Higher
        // electromagnetic orders remain present but quieter in the cabin.
        // Motor dominates the launch; tire and wind increasingly mask it after
        // the usual 20 km/h pedestrian-warning range. This is cabin motor
        // sound, not the external warning device itself.
        let launch = (1. - (self.speed / 50.).clamp(0., 1.)).powi(2);
        let motor_mask = 1. / (1. + (metres_per_second / 18.).powi(2));
        let motor_level = (0.007 + torque.abs().sqrt() * (0.021 + 0.044 * launch))
            * motor_mask
            * (metres_per_second / (metres_per_second + 0.6));
        let motor_mono = self.motor_mount.process(
            ((self.traction_phase * TAU).sin() * 0.8
                + (self.regen_phase * TAU).sin() * 0.3
                + motor_angle.sin() * 0.12)
                * motor_level,
        );
        let regen = if torque < -0.1 {
            (self.regen_phase * TAU).sin() * -torque * 0.006 * motor_mask
        } else {
            0.
        };

        self.road_distance += metres_per_second as f64 / RATE as f64;
        let contact_l = RoadProfile::contact(self.road_distance, 0x1098_43af, self.bumpiness);
        let contact_r = RoadProfile::contact(self.road_distance, 0x7a13_2d91, self.bumpiness);
        let tread_l = RoadProfile::tread(self.road_distance, 0x1098_43af);
        let tread_r = RoadProfile::tread(self.road_distance, 0x7a13_2d91);
        let roughness = RoadProfile::roughness(self.road_distance);
        let road_level =
            (metres_per_second / 12.).max(0.).powf(0.9).min(2.8) * self.texture.clamp(0.5, 1.5);
        let body_l = self.body_left[0].process(contact_l) * 0.9
            + self.body_left[1].process(contact_l) * 2.5
            + self.body_left[2].process(contact_l) * 2.0;
        let body_r = self.body_right[0].process(contact_r) * 0.9
            + self.body_right[1].process(contact_r) * 2.5
            + self.body_right[2].process(contact_r) * 2.0;
        let wet_hiss = (self.precipitation / 3.).clamp(0., 1.);
        let tire_l = (body_l * 0.72 * roughness
            + self.tire_air_left.process(tread_l) * (0.25 + wet_hiss * 0.06) * roughness.sqrt())
            * road_level;
        let tire_r = (body_r * 0.72 * roughness
            + self.tire_air_right.process(tread_r) * (0.25 + wet_hiss * 0.06) * roughness.sqrt())
            * road_level;
        let token = self.controls.road_event_token.load(Ordering::Acquire);
        if token != self.road_event_token {
            self.road_event_token = token;
            self.road_event_kind = self.controls.road_event_kind.load(Ordering::Relaxed) as u8;
            self.road_event_age = 0.;
            self.road_event_speed = metres_per_second.max(2.);
        }
        let event_age = self.road_event_age;
        self.road_event_age = (event_age + 1. / RATE).min(2.);
        let impact = |age: f32| (-age.max(0.) * 45.).exp() * f32::from(age >= 0.);
        let road_event = if self.road_event_kind == 2 {
            (0..5)
                .map(|i| impact(event_age - i as f32 * 0.095))
                .sum::<f32>()
                * 0.017
        } else if self.road_event_kind == 1 {
            (impact(event_age) + impact(event_age - 2.8 / self.road_event_speed) * 0.85) * 0.045
        } else {
            0.
        };
        let road_event = self
            .road_event_filter
            .process(self.road_event_noise.sample())
            * road_event;

        // Weather wind direction is relative to the virtual heading. Gusts are
        // correlated low-frequency stochastic pressure changes, not an LFO.
        let wind_angle = self.wind_from.to_radians();
        let headwind = self.wind / 3.6 * wind_angle.cos();
        let crosswind = self.wind / 3.6 * wind_angle.sin();
        let apparent_air = (metres_per_second + headwind)
            .max(0.)
            .hypot(crosswind)
            .max(0.);
        let wind_white = self.wind_noise.sample();
        self.gust_slow += (wind_white - self.gust_slow) * 0.000055;
        self.gust_fast += (wind_white - self.gust_fast) * 0.00065;
        let gust = (1. + self.gust_slow * 7. + self.gust_fast * 1.8).clamp(0.55, 1.8);
        let dynamic_pressure = 0.5 * 1.225 * apparent_air.powi(2);
        let wind_level = (dynamic_pressure / 383.).min(2.5) * gust;
        let side_bias = (1. - crosswind / 20.).clamp(0.6, 1.4);
        let pressure = self.wind_pressure[0].process(wind_white) * 2.2
            + self.wind_pressure[1].process(wind_white) * 0.8;
        let airborne = self
            .closed_window
            .process(self.wind_air.process(wind_white));
        let window_mono = (pressure * 0.024 + airborne * 0.012) * wind_level * side_bias;
        let windshield = (pressure * 0.014 + airborne * 0.006) * wind_level;
        let buffet = pressure * (crosswind.abs() / 20.).min(1.) * wind_level * 0.04;

        // Individual surrounding cars are synthetic. TomTom supplies traffic
        // flow, not vehicle tracks; event density follows its congestion cue.
        let mut passing_mono = 0.;
        if self.traffic > 0.02 {
            self.pass_wait -= 1. / RATE;
            if self.passing.is_none() && self.pass_wait <= 0. {
                let side = if self.traffic_noise.unit() > 0.5 {
                    1.
                } else {
                    -1.
                };
                self.passing = Some(PassBy {
                    side,
                    elapsed: 0.,
                    duration: (1.65 - metres_per_second * 0.035).clamp(0.75, 1.45),
                    strength: 0.55 + 0.45 * self.traffic_noise.unit(),
                    phase: 0.,
                });
                let cars_per_minute = 3. + self.traffic * 24.;
                self.pass_wait = 60. / cars_per_minute * (0.7 + self.traffic_noise.unit() * 0.6);
            }
        }
        if let Some(pass) = &mut self.passing {
            pass.elapsed += 1. / RATE;
            let progress = (pass.elapsed / pass.duration).min(1.);
            if self.frames.is_multiple_of(256) {
                self.other_car
                    .set_angle(pass.side * (25. + progress * 140.));
            }
            let (low, mid, _) = self.traffic_color.split(self.traffic_noise.sample());
            let approach = PassBy::envelope(progress);
            pass.phase = (pass.phase + (76. + 30. * (1. - progress)) / RATE).fract();
            let outside = self.passing_glass[0]
                .process(mid * 0.7 + low * 0.7 + (pass.phase * TAU).sin() * 0.07);
            let tire_and_body = self.passing_glass[1].process(outside);
            passing_mono = tire_and_body * approach * pass.strength * 0.13 * (1. + wet_hiss * 0.35);
            if progress >= 1. {
                self.passing = None;
            }
        }

        let recorded = self.recorded_cabin.render(metres_per_second, self.texture);
        let motor_drive = (motor_mono + regen)
            * Self::motor_presence(self.speed, self.acceleration)
            * Self::motor_burst_gain(self.motor_burst_age);
        // The brief whine is a centered cabin vibration, with no fixed
        // free-field direction or front-right reflection.
        let motor_body = motor_drive * 0.96;
        let passing = self.other_car.render(passing_mono);
        let indicator_side = self.controls.indicator.load(Ordering::Relaxed) as i32 as i8;
        if indicator_side != self.indicator_side {
            self.indicator_side = indicator_side;
            self.indicator_phase = 0.;
            self.indicator_click_age = 1.;
            self.indicator_second_click = false;
            self.indicator_first_tick = true;
            self.stalk_age = 0.;
            self.stalk_engaging = indicator_side != 0;
        }
        if self.indicator_side != 0 {
            self.indicator_phase += 1. / RATE;
            // A close on/off pair followed by a longer pause; the stalk's
            // engage/release clunk is a separate event from either tick.
            let gap = if self.indicator_first_tick {
                0.12
            } else if self.indicator_second_click {
                0.23
            } else {
                0.45
            };
            if self.indicator_phase >= gap {
                self.indicator_phase = 0.;
                self.indicator_click_age = 0.;
                self.indicator_second_click = !self.indicator_second_click;
                self.indicator_first_tick = false;
            }
        }
        let click_envelope = (-self.indicator_click_age * 115.).exp();
        self.indicator_click_age = (self.indicator_click_age + 1. / RATE).min(1.);
        let indicator_click = self.indicator_filter.process(self.indicator_noise.sample())
            * click_envelope
            * if self.indicator_second_click {
                0.11
            } else {
                0.15
            };
        let stalk_envelope = (-self.stalk_age * 85.).exp();
        let stalk = self.stalk_filter.process(self.stalk_noise.sample())
            * stalk_envelope
            * if self.stalk_engaging { 0.44 } else { 0.37 };
        self.stalk_age = (self.stalk_age + 1. / RATE).min(1.);
        let indicator = indicator_click + stalk;
        // The fan and distant road wash persist at a stop. Independent vent
        // noise gives width without making a steady sound seem left-panned.
        let air_l = self.cabin_air_left.sample();
        let air_r = self.cabin_air_right.sample();
        let low_air = self.cabin_low_air.process((air_l + air_r) * 0.5);
        self.fan_phase = (self.fan_phase + (92. + low_air * 35.) / RATE).fract();
        let fan =
            (self.fan_phase * TAU).sin() * 0.00045 + (self.fan_phase * TAU * 2.).sin() * 0.00015;
        let distant = self.distant_traffic.process(self.traffic_noise.sample())
            * (0.4 + self.traffic * 0.6)
            * 0.006;
        let vent_l = self.cabin_air_filters[0].process(air_l) * 0.023;
        let vent_r = self.cabin_air_filters[1].process(air_r) * 0.023;
        let room_tone = [
            vent_l * 0.55 + fan * 0.7 + distant,
            vent_r * 0.55 + fan * 0.7 + distant,
        ];
        let vent_source = (vent_l + vent_r) * 0.5 + fan;
        let rain_level = (self.precipitation / 2.).clamp(0., 1.);
        let rain_sample = self.rain_noise.sample();
        let mut windshield_rain = [0.; 2];
        if rain_level > 0.12 {
            let cycles_per_second = 0.55 + rain_level * 0.65;
            self.wiper_phase = (self.wiper_phase + cycles_per_second / RATE).fract();
            let phase = self.wiper_phase;
            let (sweep, right) = if phase < 0.43 {
                (
                    (std::f32::consts::PI * phase / 0.43).sin().max(0.),
                    phase / 0.43,
                )
            } else if phase < 0.86 {
                (
                    (std::f32::consts::PI * (phase - 0.43) / 0.43).sin().max(0.),
                    1. - (phase - 0.43) / 0.43,
                )
            } else {
                (0., 0.5)
            };
            let blade =
                self.wiper_filter.process(rain_sample) * sweep * (0.002 + rain_level * 0.006);
            windshield_rain[0] += blade * (0.75 - right * 0.25);
            windshield_rain[1] += blade * (0.5 + right * 0.25);
            let rate = 2. + rain_level * 12.;
            if self.rain_drop_age >= 0.08 && self.rain_noise.unit() < rate / RATE {
                self.rain_drop_age = 0.;
                self.rain_drop_right = self.rain_noise.unit() > 0.5;
            }
            let drop = self.rain_filter.process(self.rain_noise.sample())
                * (-self.rain_drop_age * 130.).exp()
                * rain_level
                * 0.014;
            windshield_rain[usize::from(self.rain_drop_right)] += drop;
            windshield_rain[usize::from(!self.rain_drop_right)] += drop * 0.45;
            self.rain_drop_age = (self.rain_drop_age + 1. / RATE).min(1.);
        }
        // Road force and sealed-window pressure reach the ears through the
        // whole body/cabin. Keep only a little left/right tire-track detail;
        // moving cars retain distinct source locations.
        let road_common = (tire_l + tire_r) * 0.5 + road_event;
        let wind_common = window_mono + buffet + windshield;
        let wind_balance = (crosswind / 20.).clamp(-1., 1.) * 0.035;
        let mut direct = [
            road_common * 0.75
                + tire_l * 0.17
                + wind_common * (0.75 - wind_balance)
                + motor_body
                + room_tone[0]
                + windshield_rain[0]
                + indicator * 0.32
                + passing[0],
            road_common * 0.75
                + tire_r * 0.17
                + wind_common * (0.75 + wind_balance)
                + motor_body
                + room_tone[1]
                + windshield_rain[1]
                + indicator * 0.32
                + passing[1],
        ];
        direct[0] = direct[0] * 0.68 + recorded[0] * 0.72;
        direct[1] = direct[1] * 0.68 + recorded[1] * 0.72;
        let reflection_source = (tire_l + tire_r) * 0.5
            + road_event * 0.7
            + (window_mono + buffet + windshield) * 0.4
            + passing_mono * 0.3
            + indicator * 0.85
            + (windshield_rain[0] + windshield_rain[1]) * 0.35
            + (recorded[0] + recorded[1]) * 0.50;
        let cabin = self.cabin.render(direct, reflection_source, vent_source);
        self.frames += 1;
        [
            (cabin[0] * self.gain * self.volume).tanh(),
            (cabin[1] * self.gain * self.volume).tanh(),
        ]
    }
}
impl Iterator for EvSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.next_is_right {
            self.next_is_right = false;
            Some(self.right_sample)
        } else {
            let frame = self.render_frame();
            self.right_sample = frame[1];
            self.next_is_right = true;
            Some(frame[0])
        }
    }
}
impl Source for EvSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> NonZero<u16> {
        NonZero::new(2).unwrap()
    }
    fn sample_rate(&self) -> NonZero<u32> {
        NonZero::new(44_100).unwrap()
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn passing_car_is_a_short_muffled_whoosh() {
        assert_eq!(PassBy::envelope(0.5), 1.);
        assert!(PassBy::envelope(0.) < 0.01);
        assert!(PassBy::envelope(1.) < 0.01);
        assert!(PassBy::envelope(0.2) < 0.2);
        let transmitted = |frequency: f32| {
            let mut glass = [Biquad::lowpass(650.), Biquad::lowpass(380.)];
            let mut energy = 0.;
            for sample in 0..12_000 {
                let input = (TAU * frequency * sample as f32 / RATE).sin();
                let first = glass[0].process(input);
                let output = glass[1].process(first);
                if sample >= 4_000 {
                    energy += output * output;
                }
            }
            energy
        };
        assert!(transmitted(3000.) < transmitted(250.) * 0.001);
    }
    #[test]
    fn indicator_has_close_tick_pair_and_diffuse_cabin_image() {
        let controls = Arc::new(EvControls::default());
        controls.set(EvInput {
            volume: 0.7,
            indicator: -1,
            active: true,
            ..EvInput::default()
        });
        let mut source = EvSource::new(controls);
        let mut pair = [0.; 2];
        let mut difference = 0.;
        for frame in 0..(RATE as usize * 3 / 4) {
            let left = source.next().unwrap();
            let right = source.next().unwrap();
            if (0.34 * RATE) as usize <= frame && frame < (0.39 * RATE) as usize {
                pair[0] += left * left;
                pair[1] += right * right;
                difference += (left - right).abs();
            }
        }
        assert!(pair[0] > 0.0001 && pair[1] > 0.0001);
        assert!((pair[0] / pair[1]).clamp(0., 10.) < 1.3);
        assert!(difference > 0.01);
    }
    #[test]
    fn stopped_cabin_keeps_a_stereo_room_tone() {
        let controls = Arc::new(EvControls::default());
        controls.set(EvInput {
            speed_kmh: 0.,
            volume: 0.7,
            traffic_density: 0.6,
            active: true,
            ..EvInput::default()
        });
        let mut source = EvSource::new(controls);
        source.by_ref().take(44_100).for_each(drop);
        let mut energy = [0.; 2];
        let mut difference = 0.;
        for _ in 0..44_100 {
            let left = source.next().unwrap();
            let right = source.next().unwrap();
            energy[0] += left * left;
            energy[1] += right * right;
            difference += (left - right).abs();
        }
        assert!(energy[0] > 0.005 && energy[1] > 0.005);
        assert!(difference > 10.);
    }
    #[test]
    fn rain_starts_wipers_and_mapped_hump_excites_body() {
        let controls = Arc::new(EvControls::default());
        controls.set(EvInput {
            speed_kmh: 35.,
            volume: 0.7,
            precipitation_mm: 2.,
            active: true,
            ..EvInput::default()
        });
        let mut source = EvSource::new(controls.clone());
        for _ in 0..RATE as usize {
            source.render_frame();
        }
        assert!(source.wiper_phase > 0.);
        controls.set(EvInput {
            speed_kmh: 35.,
            volume: 0.7,
            precipitation_mm: 2.,
            road_event_kind: 1,
            road_event_token: 1,
            active: true,
            ..EvInput::default()
        });
        let mut peak: f32 = 0.;
        for _ in 0..(RATE as usize / 4) {
            let frame = source.render_frame();
            peak = peak.max(frame[0].abs()).max(frame[1].abs());
        }
        assert_eq!(source.road_event_token, 1);
        assert!(peak > 0.001);
    }
    #[test]
    fn closed_glass_attenuates_treble_wind() {
        let transmitted = |frequency: f32| {
            let mut outside = Biquad::lowpass(650.);
            let mut glass = Biquad::lowpass(420.);
            let mut energy = 0.;
            for sample in 0..12_000 {
                let input = (TAU * frequency * sample as f32 / RATE).sin();
                let output = glass.process(outside.process(input));
                if sample >= 4_000 {
                    energy += output * output;
                }
            }
            energy
        };
        assert!(transmitted(3000.) < transmitted(250.) * 0.001);
    }
    #[test]
    fn cabin_has_directional_early_reflections() {
        assert_ne!(Hrtf::cabin_reflection(0), Hrtf::cabin_reflection(1));
        let mut cabin = Cabin::new();
        let mut early_energy = [0.; 2];
        for frame in 0..800 {
            let reflected = cabin.render([0.; 2], f32::from(frame == 0), 0.);
            if frame > 100 {
                early_energy[0] += reflected[0].abs();
                early_energy[1] += reflected[1].abs();
            }
        }
        assert!(early_energy[0] > 0.001);
        assert!(early_energy[1] > 0.001);
    }
    #[test]
    fn dashboard_vents_create_short_binaural_reflections() {
        let mut cabin = Cabin::new();
        let mut energy = [0.; 2];
        for frame in 0..450 {
            let output = cabin.render([0.; 2], 0., f32::from(frame == 0));
            if frame > 75 {
                energy[0] += output[0].abs();
                energy[1] += output[1].abs();
            }
        }
        assert!(energy[0] > 0.001 && energy[1] > 0.001);
    }
    #[test]
    fn road_profile_is_fixed_in_space_and_continuous_through_origin() {
        let side = 0x1098_43af;
        let x = 12.345;
        assert_eq!(RoadProfile::height(x, side), RoadProfile::height(x, side));
        assert!(
            (RoadProfile::height(-0.00001, side) - RoadProfile::height(0.00001, side)).abs() < 0.01
        );
        assert!((RoadProfile::height(x, side) - RoadProfile::height(x + 0.01, side)).abs() < 0.2);
        assert!((RoadProfile::height(x, side) - RoadProfile::height(x, 0x7a13_2d91)).abs() > 0.001);
    }
    #[test]
    fn measured_hrtf_positions_right_and_left() {
        let energy = |angle| {
            let taps = Hrtf::taps(angle);
            taps.map(|ear| ear.iter().map(|sample| sample * sample).sum::<f32>())
        };
        let right = energy(90.);
        let left = energy(-90.);
        assert!(right[1] > right[0] * 3.);
        assert!(left[0] > left[1] * 3.);
        assert!((right[0] - left[1]).abs() < 0.00001);
    }
    #[test]
    fn binaural_cabin_is_stereo_bounded_and_speed_sensitive() {
        let controls = Arc::new(EvControls::default());
        controls.set(EvInput {
            speed_kmh: 85.,
            acceleration_mps2: 1.2,
            road_texture: 1.,
            bumpiness: 0.3,
            volume: 0.7,
            wind_kmh: 15.,
            traffic_density: 0.8,
            active: true,
            ..EvInput::default()
        });
        let mut source = EvSource::new(controls);
        assert_eq!(source.channels().get(), 2);
        let samples: Vec<f32> = source.by_ref().take(44_100 * 2).collect();
        let (mut difference, mut energy, mut left_energy, mut right_energy) = (0., 0., 0., 0.);
        for pair in samples.chunks_exact(2) {
            difference += (pair[0] - pair[1]).abs();
            energy += pair[0].abs() + pair[1].abs();
            left_energy += pair[0] * pair[0];
            right_energy += pair[1] * pair[1];
            assert!(pair
                .iter()
                .all(|value| value.is_finite() && value.abs() <= 1.));
        }
        assert!(energy > 10.);
        assert!(difference > energy * 0.1);
        assert!(left_energy / right_energy > 0.8);
        assert!(left_energy / right_energy < 1.2);
    }
    #[test]
    fn long_road_sections_change_rumble_smoothly() {
        let values: Vec<f32> = (0..1000)
            .map(|index| RoadProfile::roughness(index as f64 * 0.1))
            .collect();
        let low = values.iter().copied().fold(f32::INFINITY, f32::min);
        let high = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        assert!(high - low > 0.25);
        assert!(values
            .windows(2)
            .all(|pair| (pair[1] - pair[0]).abs() < 0.05));
    }
    #[test]
    fn road_defects_are_repeatable_and_more_frequent_on_sparse_roads() {
        let count = |bumpiness| {
            (0..5_000)
                .filter(|i| {
                    RoadProfile::defect(*i as f64 * 0.2, 0x1098_43af, bumpiness).abs() > 0.02
                })
                .count()
        };
        assert!(count(0.9) > count(0.1));
        assert_eq!(
            RoadProfile::defect(123.4, 0x1098_43af, 0.9),
            RoadProfile::defect(123.4, 0x1098_43af, 0.9)
        );
    }
    #[test]
    fn cabin_volume_zero_silences_output() {
        let controls = Arc::new(EvControls::default());
        controls.set(EvInput {
            speed_kmh: 60.,
            volume: 0.,
            active: true,
            ..EvInput::default()
        });
        let mut source = EvSource::new(controls);
        assert!(source.by_ref().take(2_000).all(|sample| sample == 0.));
    }
    #[test]
    fn motor_is_foreground_at_launch_and_recedes_at_cruise() {
        let launch = EvSource::motor_presence(5., 1.7);
        let cruise = EvSource::motor_presence(60., 0.);
        assert!(launch > cruise * 3.);
        assert!(EvSource::motor_presence(5., 0.) < launch);
    }
    #[test]
    fn motor_whine_stays_audible_and_fades_after_acceleration() {
        assert_eq!(EvSource::motor_burst_gain(0.), 1.2);
        assert!((EvSource::motor_burst_gain(0.75) - 0.6).abs() < 0.0001);
        assert_eq!(EvSource::motor_burst_gain(1.5), 0.);

        let controls = Arc::new(EvControls::default());
        controls.set(EvInput {
            speed_kmh: 10.,
            acceleration_mps2: 1.7,
            volume: 0.7,
            active: true,
            ..EvInput::default()
        });
        let mut source = EvSource::new(controls.clone());
        source.by_ref().take(44_100 * 2).for_each(drop);
        assert!((0.9..1.1).contains(&source.motor_burst_age));
        controls.set(EvInput {
            speed_kmh: 10.,
            acceleration_mps2: 0.,
            volume: 0.7,
            active: true,
            ..EvInput::default()
        });
        source.by_ref().take(44_100).for_each(drop);
        controls.set(EvInput {
            speed_kmh: 10.,
            acceleration_mps2: 1.7,
            volume: 0.7,
            active: true,
            ..EvInput::default()
        });
        source.by_ref().take(44_100 / 5).for_each(drop);
        assert!(source.motor_burst_age < 0.3);
    }
    #[test]
    #[ignore = "Renders a reference stereo drive and stop to target/ev-cabin-demo.wav"]
    fn render_reference_drive() {
        use std::{fs::File, io::Write};
        let controls = Arc::new(EvControls::default());
        let mut source = EvSource::new(controls.clone());
        let frames = 25 * 44_100u32;
        let mut pcm = Vec::with_capacity(frames as usize * 4);
        for frame in 0..frames {
            if frame % 882 == 0 {
                let second = frame as f32 / RATE;
                let speed = if second < 10. {
                    second * 5.
                } else if second < 14. {
                    50.
                } else if second < 22. {
                    (50. - (second - 14.) * 6.25).max(0.)
                } else {
                    0.
                };
                controls.set(EvInput {
                    speed_kmh: speed,
                    acceleration_mps2: if second < 10. {
                        1.4
                    } else if second < 14. {
                        0.
                    } else if second < 22. {
                        -1.7
                    } else {
                        0.
                    },
                    grade: 0.,
                    road_texture: 1.05,
                    bumpiness: 0.3,
                    volume: 0.7,
                    wind_kmh: 25.,
                    wind_from_deg: -70.,
                    traffic_density: 0.8,
                    precipitation_mm: 0.5,
                    road_event_kind: if second >= 17. {
                        2
                    } else if second >= 7. {
                        1
                    } else {
                        0
                    },
                    road_event_token: if second >= 17. {
                        2
                    } else if second >= 7. {
                        1
                    } else {
                        0
                    },
                    indicator: if (9. ..14.).contains(&second) { 1 } else { 0 },
                    active: true,
                });
            }
            for _ in 0..2 {
                let sample = source.next().unwrap();
                pcm.extend_from_slice(&((sample * 32767.) as i16).to_le_bytes());
            }
        }
        let path = "target/ev-cabin-demo.wav";
        let mut file = File::create(path).unwrap();
        file.write_all(b"RIFF").unwrap();
        file.write_all(&(36 + pcm.len() as u32).to_le_bytes())
            .unwrap();
        file.write_all(b"WAVEfmt ").unwrap();
        file.write_all(&16u32.to_le_bytes()).unwrap();
        file.write_all(&1u16.to_le_bytes()).unwrap();
        file.write_all(&2u16.to_le_bytes()).unwrap();
        file.write_all(&44_100u32.to_le_bytes()).unwrap();
        file.write_all(&(44_100u32 * 4).to_le_bytes()).unwrap();
        file.write_all(&4u16.to_le_bytes()).unwrap();
        file.write_all(&16u16.to_le_bytes()).unwrap();
        file.write_all(b"data").unwrap();
        file.write_all(&(pcm.len() as u32).to_le_bytes()).unwrap();
        file.write_all(&pcm).unwrap();
    }
}
