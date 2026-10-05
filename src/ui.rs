use crate::{
    audio::Audio,
    data::{self, HighwayPreference, Network, SearchResult, TrafficFlow, TrafficIncident, Weather},
    ev_audio::EvInput,
    geo::Point,
    map::{AddressContext, Area, FallbackSpeeds, Place, PreviewRoad},
    navigation::{Announcer, Guidance, Route},
    speech::Speech,
};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::{HashSet, VecDeque},
    ptr::null_mut,
    sync::mpsc::{self, Receiver, Sender},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::LibraryLoader::*,
    UI::{
        Controls::EM_SETSEL, Input::KeyboardAndMouse::*, Shell::ShellExecuteW,
        WindowsAndMessaging::*,
    },
};

const SEARCH: i32 = 101;
const FIND: i32 = 102;
const RESULTS: i32 = 103;
const LOAD: i32 = 104;
const WALK: i32 = 105;
const STATUS: i32 = 106;
const ROUTE: i32 = 107;
const HEADING: i32 = 108;
const TURN: i32 = 109;
const FACE: i32 = 110;
const APPLY: i32 = 111;
const FIX_MAP: i32 = 112;
const MY_LOCATION: i32 = 113;
const AROUND_ME: i32 = 114;
const AHEAD_OF_ME: i32 = 115;
const NEARBY: i32 = 116;
const SEARCH_ACTION: i32 = 117;
const ROUTE_ACTION: i32 = 118;
const BEACON_ACTION: i32 = 119;
const ROUTE_FOLLOW: i32 = 120;
const ROUTE_RECALC: i32 = 121;
const ROUTE_CANCEL: i32 = 122;
const ROUTE_BEACON: i32 = 123;
const SAVED_ACTION: i32 = 124;
const STREET_PREVIEW: i32 = 125;
const PAGE_PREVIOUS: i32 = 126;
const PAGE_NEXT: i32 = 127;
const PLACE_INFO: i32 = 128;
const DRIVE_TOUR: i32 = 129;
const WEATHER_ACTION: i32 = 130;
const DRIVE_OPTIONS: i32 = 131;
const OPTIONS_NEXT: i32 = 132;
const OPTIONS_PREV: i32 = 133;
const PAGE_SIZE: usize = 10;
enum Job {
    SetGoogleMode(bool),
    Search(String, Point),
    Load(SearchResult),
    Cover(Point, u64),
    Route(Point, SearchResult, u64),
    WhereAmI(Point, f64),
    PlaceContext(Place, bool),
    DriveRoute(Point, SearchResult, HighwayPreference, u64),
    Weather(Point, bool),
    Elevations(Vec<(f64, Point)>, u64),
    Traffic(Point, f64, u64),
    Incidents(Point, u64),
    DriveAddress(Point, u64),
    GoogleWalkAddress(Point, u64),
}
enum Reply {
    Search(Result<Vec<SearchResult>, String>),
    Load(Result<(Area, Point), String>),
    Cover(Result<Area, String>, u64),
    Route(Result<Route, String>, Point, u64),
    PcLocation(Result<Point, String>, Point),
    WhereAmI(Result<AddressContext, String>, Point, f64),
    PlaceContext(String, Result<(String, String), String>, bool),
    DriveRoute(Result<Route, String>, Point, u64),
    Weather(Result<Weather, String>, Point, bool),
    Elevations(Result<Vec<f64>, String>, Vec<f64>, u64),
    Traffic(Result<TrafficFlow, String>, Point, f64, u64),
    Incidents(Result<Vec<TrafficIncident>, String>, u64),
    DriveAddress(Result<AddressContext, String>, Point, u64),
    GoogleWalkAddress(Result<AddressContext, String>, Point, u64),
}
#[derive(Serialize, Deserialize)]
struct Preferences {
    start_at_pc_location: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct DrivingSettings {
    route_preference: HighwayPreference,
    signal_wait_seconds: u8,
    live_traffic: bool,
    fallback: FallbackSpeeds,
    google_mode: bool,
    ev_audio: bool,
    ev_volume: u8,
}
impl Default for DrivingSettings {
    fn default() -> Self {
        Self {
            route_preference: HighwayPreference::Fastest,
            signal_wait_seconds: 0,
            live_traffic: true,
            fallback: FallbackSpeeds::default(),
            google_mode: false,
            ev_audio: true,
            ev_volume: 70,
        }
    }
}
impl DrivingSettings {
    fn valid(&self) -> bool {
        self.signal_wait_seconds <= 30
            && self.ev_volume <= 100
            && [
                self.fallback.built_up,
                self.fallback.suburban,
                self.fallback.rural,
                self.fallback.open_road,
            ]
            .iter()
            .all(|v| v.is_finite() && (10. ..=130.).contains(v))
    }
    fn labels(&self) -> Vec<String> {
        vec![
            format!("Route: {}", self.route_preference.label()),
            format!(
                "Simulated signal wait: {}",
                if self.signal_wait_seconds == 0 {
                    "Off".into()
                } else {
                    format!("{} seconds", self.signal_wait_seconds)
                }
            ),
            format!(
                "Live traffic: {}",
                if self.live_traffic { "On" } else { "Off" }
            ),
            format!("Built-up fallback: {:.0} km/h", self.fallback.built_up),
            format!("Suburban fallback: {:.0} km/h", self.fallback.suburban),
            format!("Rural fallback: {:.0} km/h", self.fallback.rural),
            format!("Open-road fallback: {:.0} km/h", self.fallback.open_road),
            format!(
                "Map and route source: {}",
                if self.google_mode {
                    "Google Maps"
                } else {
                    "OpenStreetMap"
                }
            ),
            format!(
                "EV cabin sound: {}",
                if self.ev_audio { "On" } else { "Off" }
            ),
            format!("EV cabin volume: {} percent", self.ev_volume),
        ]
    }
    fn adjust(&mut self, index: usize, direction: i32) {
        match index {
            0 => self.route_preference = self.route_preference.next(direction),
            1 => {
                self.signal_wait_seconds =
                    ((self.signal_wait_seconds as i32 + 5 * direction).rem_euclid(35)) as u8
            }
            2 => self.live_traffic = !self.live_traffic,
            3..=6 => {
                let speed = match index {
                    3 => &mut self.fallback.built_up,
                    4 => &mut self.fallback.suburban,
                    5 => &mut self.fallback.rural,
                    _ => &mut self.fallback.open_road,
                };
                *speed = (*speed + 5. * direction as f64 - 10.).rem_euclid(125.) + 10.;
            }
            7 => self.google_mode = !self.google_mode,
            8 => self.ev_audio = !self.ev_audio,
            9 => self.ev_volume = (self.ev_volume as i32 + direction * 10).clamp(0, 100) as u8,
            _ => {}
        }
    }
}
#[derive(Serialize, Deserialize)]
struct Session {
    area: Area,
    point: Point,
    heading: f64,
    step: usize,
    #[serde(default = "default_turn")]
    turn: f64,
    #[serde(default)]
    route: Option<Route>,
}
fn default_turn() -> f64 {
    15.
}
struct App {
    hwnd: HWND,
    view_title: HWND,
    search: HWND,
    results: HWND,
    walk: HWND,
    output: HWND,
    map_attribution: HWND,
    route_attribution: HWND,
    route_title: HWND,
    route_list: HWND,
    page_previous: HWND,
    page_next: HWND,
    heading_input: HWND,
    turn_input: HWND,
    area: Area,
    point: Point,
    heading: f64,
    step: usize,
    history: Vec<(Point, f64)>,
    choices: Vec<SearchResult>,
    page: usize,
    route_page: usize,
    preview_options: Vec<PreviewRoad>,
    browse_mode: BrowseMode,
    categories: Vec<String>,
    speech: Speech,
    audio: Audio,
    muted: bool,
    busy: bool,
    tx: Sender<Job>,
    map_tx: Sender<Job>,
    rx: Receiver<Reply>,
    location_tx: Sender<Reply>,
    start_at_pc_location: bool,
    announcer: Announcer,
    route: Option<Guidance>,
    drive: Option<Drive>,
    drive_generation: u64,
    traffic_generation: u64,
    driving_settings: DrivingSettings,
    weather_pending: bool,
    traffic_pending: bool,
    last_traffic_request: Option<(Point, Instant)>,
    last_traffic: Option<TrafficFlow>,
    traffic_error_reported: bool,
    incidents_pending: bool,
    last_incidents_request: Option<(Point, Instant)>,
    incidents: Vec<TrafficIncident>,
    incidents_error_reported: bool,
    drive_address_pending: bool,
    last_drive_address_request: Option<(Point, Instant)>,
    pending_drive_address: Option<String>,
    google_walk_pending: bool,
    last_google_walk_request: Option<(Point, Instant)>,
    last_weather_request: Option<(Point, Instant)>,
    last_weather: Option<Weather>,
    turn: f64,
    destination_mode: bool,
    spare: Vec<Area>,
    map_generation: u64,
    route_generation: u64,
    cover_pending: bool,
    last_cover: Option<Instant>,
    map_error: bool,
    last_walk: Option<Instant>,
    view: View,
    search_controls: Vec<Positioned>,
    results_controls: Vec<Positioned>,
    walking_controls: Vec<Positioned>,
    route_controls: Vec<Positioned>,
    settings_controls: Vec<Positioned>,
    announcements: RefCell<VecDeque<String>>,
}
struct Drive {
    route: Route,
    progress: f64,
    last_tick: Instant,
    last_callout: Instant,
    last_poi_scan: Instant,
    last_context_callout: Instant,
    mentioned: HashSet<String>,
    paused: bool,
    paused_by_closure: bool,
    speed_kmh: f64,
    traffic_kmh: Option<f64>,
    lead: Option<LeadTraffic>,
    actual_speed_kmh: f64,
    actual_acceleration_mps2: f64,
    driver: DriverProfile,
    departure_delay: f64,
    texture: f64,
    tagged_speed: bool,
    last_speed_callout: Instant,
    last_speed_check: Instant,
    stop: Option<TrafficStop>,
    seen_road_events: HashSet<String>,
    road_event_counter: u32,
    road_event_kind: u8,
    elevation_samples: Vec<(f64, f64)>,
    elevation_pending: bool,
    elevation_until: f64,
    seen_signals: HashSet<String>,
    mentioned_incidents: HashSet<String>,
    last_incident_callout: Instant,
    next_maneuver: usize,
    maneuver_previewed: bool,
    last_street_context: Option<String>,
    last_street_announcement: Instant,
    last_street_speech: Instant,
    last_guidance_announcement: Instant,
    last_guidance_progress: f64,
}
impl Drive {
    fn new(
        route: Route,
        speed_kmh: f64,
        tagged_speed: bool,
        texture: f64,
        start_context: Option<String>,
    ) -> Self {
        let now = Instant::now();
        let driver = DriverProfile::sampled();
        let next_maneuver = route
            .maneuvers
            .iter()
            .position(|m| m.at > 10.)
            .unwrap_or(route.maneuvers.len());
        Self {
            route,
            progress: 0.,
            last_tick: now,
            last_callout: now - Duration::from_secs(8),
            last_poi_scan: now - Duration::from_secs(1),
            last_context_callout: now - Duration::from_secs(30),
            mentioned: HashSet::new(),
            paused: false,
            paused_by_closure: false,
            speed_kmh,
            traffic_kmh: None,
            lead: None,
            actual_speed_kmh: 0.,
            actual_acceleration_mps2: 0.,
            driver,
            departure_delay: driver.reaction_seconds,
            texture,
            tagged_speed,
            last_speed_callout: now - Duration::from_secs(12),
            last_speed_check: now,
            stop: None,
            seen_signals: HashSet::new(),
            seen_road_events: HashSet::new(),
            road_event_counter: 0,
            road_event_kind: 0,
            elevation_samples: vec![],
            elevation_pending: false,
            elevation_until: 0.,
            mentioned_incidents: HashSet::new(),
            last_incident_callout: now - Duration::from_secs(15),
            next_maneuver,
            maneuver_previewed: false,
            last_street_context: start_context,
            last_street_announcement: now,
            last_street_speech: now,
            last_guidance_announcement: now - Duration::from_secs(5),
            last_guidance_progress: 0.,
        }
    }
    fn clear_live_traffic(&mut self) {
        self.traffic_kmh = None;
        self.lead = None;
        if self.paused_by_closure {
            self.paused = false;
            self.paused_by_closure = false;
            self.last_tick = Instant::now();
        }
    }
}
fn route_grade(samples: &[(f64, f64)], progress: f64) -> f64 {
    let interpolate = |at: f64| {
        samples
            .windows(2)
            .find(|pair| pair[0].0 <= at && pair[1].0 >= at)
            .map(|pair| {
                let fraction = ((at - pair[0].0) / (pair[1].0 - pair[0].0).max(1.)).clamp(0., 1.);
                pair[0].1 + (pair[1].1 - pair[0].1) * fraction
            })
    };
    interpolate(progress)
        .zip(interpolate(progress + 150.))
        .map_or(0., |(a, b)| ((b - a) / 150.).clamp(-0.08, 0.08))
}
#[derive(Clone, Copy)]
struct DriverProfile {
    acceleration: f64,
    braking: f64,
    jerk: f64,
    corner_factor: f64,
    cruise_factor: f64,
    reaction_seconds: f64,
    coast_deceleration: f64,
    headway_seconds: f64,
}
impl DriverProfile {
    fn sampled() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        Self::from_seed(seed)
    }
    fn from_seed(mut seed: u64) -> Self {
        // A persistent latent style correlates throttle, braking and corner
        // choices. Small independent draws keep identical styles from feeling
        // scripted. These are bounded plausible ranges, not a fit to raw trips.
        let mut draw = || {
            seed = seed.wrapping_add(0x9e3779b97f4a7c15);
            let mut x = seed;
            x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
            x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
            ((x ^ (x >> 31)) >> 11) as f64 / (1u64 << 53) as f64
        };
        let style = (draw() + draw() + draw()) / 3.;
        let jitter = draw() - 0.5;
        Self {
            acceleration: 1.05 + style * 0.8 + jitter * 0.12,
            braking: 1.25 + style * 0.85 + (draw() - 0.5) * 0.15,
            jerk: 1.5 + style * 1.0,
            corner_factor: 0.82 + style * 0.37 + (draw() - 0.5) * 0.08,
            cruise_factor: 0.87 + style * 0.13,
            reaction_seconds: 1.25 - style * 0.75 + (draw() - 0.5) * 0.18,
            coast_deceleration: 0.3 + style * 0.3,
            headway_seconds: 2.5 - style * 0.8 + (draw() - 0.5) * 0.2,
        }
    }
    fn corner_variation(self, index: usize) -> f64 {
        let hash = (index as u64).wrapping_mul(0x9e3779b97f4a7c15);
        let fraction = ((hash ^ (hash >> 32)) & 0xffff) as f64 / 65535.;
        self.corner_factor * (0.94 + fraction * 0.12)
    }
}
struct LeadTraffic {
    at: f64,
    speed_kmh: f64,
    phase: f64,
}
impl LeadTraffic {
    fn new(progress: f64, ego_kmh: f64, headway: f64) -> Self {
        Self {
            at: progress + 12. + ego_kmh / 3.6 * headway,
            speed_kmh: ego_kmh,
            phase: 0.,
        }
    }
    fn advance(
        &mut self,
        elapsed: f64,
        progress: f64,
        ego_kmh: f64,
        flow_kmh: f64,
        headway: f64,
    ) -> f64 {
        self.phase += elapsed;
        // A virtual lead vehicle provides time-correlated stop/go motion. Its
        // phase and position are synthetic; TomTom only supplies mean speed.
        let wave = (0.62 + 0.55 * (self.phase * 0.18).sin()).clamp(0., 1.18);
        let desired = flow_kmh * wave;
        self.speed_kmh += (desired - self.speed_kmh) * (elapsed / 2.).clamp(0., 1.);
        self.at += self.speed_kmh / 3.6 * elapsed;
        if self.at - progress > 240. {
            self.at = progress + 30. + ego_kmh / 3.6 * headway;
        }
        let gap = (self.at - progress - 5.).max(0.);
        let desired_gap = 5. + ego_kmh / 3.6 * headway;
        (self.speed_kmh + (gap - desired_gap) * 1.7).max(0.)
    }
}
fn drive_poi_ready_after_street(now: Instant, last_street_speech: Instant) -> bool {
    now.duration_since(last_street_speech) >= Duration::from_millis(2_500)
}
struct TrafficStop {
    at: f64,
    remaining: f64,
    reached: bool,
}
fn leaf_available_acceleration(current: f64) -> f64 {
    // Approximate a 2017 LEAF (EM57, 80 kW, 254 Nm, 8.1938:1 reduction).
    // The 1,520 kg mass represents a midrange equipment configuration.
    let metres_per_second = current / 3.6;
    let torque_limited_force: f64 = 254. * 8.1938 * 0.9 / 0.316;
    let power_limited_force = 80_000. * 0.9 / metres_per_second.max(1.);
    let motor_force = torque_limited_force.min(power_limited_force);
    let drag_force = 0.5 * 1.2 * 0.64 * metres_per_second.powi(2) + 1520. * 9.81 * 0.011;
    ((motor_force - drag_force) / 1520.).clamp(0., 3.0)
}
#[cfg(test)]
fn approach_speed(
    current: f64,
    target: f64,
    elapsed: f64,
    previous_acceleration: f64,
    driver: DriverProfile,
) -> (f64, f64) {
    approach_speed_on_grade(current, target, elapsed, previous_acceleration, driver, 0.)
}
fn approach_speed_on_grade(
    current: f64,
    target: f64,
    elapsed: f64,
    previous_acceleration: f64,
    driver: DriverProfile,
    grade: f64,
) -> (f64, f64) {
    // A driver normally asks for less than the motor can deliver. Limit jerk
    // so launch and braking build over time instead of stepping instantly.
    let comfortable_acceleration = (driver.acceleration - current / 100. * 0.7).max(0.55);
    let desired = if target > current + 0.3 {
        (leaf_available_acceleration(current) - 9.81 * grade)
            .max(0.)
            .min(comfortable_acceleration)
    } else if target < current - 0.3 {
        if current - target < 9. {
            -driver.coast_deceleration
        } else {
            -driver.braking
        }
    } else {
        0.
    };
    let acceleration = previous_acceleration
        + (desired - previous_acceleration).clamp(-driver.jerk * elapsed, driver.jerk * elapsed);
    let next = (current + acceleration * 3.6 * elapsed).max(0.);
    let next = if (current <= target && next > target) || (current >= target && next < target) {
        target
    } else {
        next
    };
    (next, (next - current) / (3.6 * elapsed.max(0.001)))
}
fn estimated_traffic_activity(urbanity: f64, flow: Option<&TrafficFlow>) -> f32 {
    // TomTom Flow has speeds, not vehicle counts. Combine a settlement proxy
    // with confidence-weighted slowdown; the audio maps this to cars/minute.
    let congestion = flow.map_or(0., |flow| {
        if flow.confidence < 0.4 || flow.closed {
            0.
        } else {
            (1. - flow.current_kmh / flow.free_kmh.max(1.))
                .clamp(0., 1.)
                .powf(0.7)
                * flow.confidence
        }
    });
    (0.08 + urbanity.clamp(0., 1.) * 0.58 + congestion * 0.34).clamp(0.08, 0.95) as f32
}
fn turn_indicator_side(route: &Route, progress: f64, speed_kmh: f64) -> i8 {
    let lookahead = (speed_kmh / 3.6 * 5.).clamp(35., 140.);
    for (index, maneuver) in route.maneuvers.iter().enumerate().skip(1) {
        let remaining = maneuver.at - progress;
        if remaining < -5. || remaining > lookahead {
            continue;
        }
        if maneuver.text.starts_with("Arrive") {
            continue;
        }
        let delta =
            crate::navigation::turn_delta(route.maneuvers[index - 1].bearing, maneuver.bearing);
        if (30. ..=160.).contains(&delta.abs()) {
            return if delta < 0. { -1 } else { 1 };
        }
    }
    0
}

fn maneuver_speed_cap(route: &Route, progress: f64, driver: DriverProfile) -> f64 {
    let mut cap = f64::INFINITY;
    for (index, maneuver) in route.maneuvers.iter().enumerate().skip(1) {
        let remaining = maneuver.at - progress;
        if remaining < -15. {
            continue;
        }
        if remaining > 900. {
            break;
        }
        let previous = &route.maneuvers[index - 1];
        let angle = crate::navigation::turn_delta(previous.bearing, maneuver.bearing).abs();
        let text = maneuver.text.to_ascii_lowercase();
        let corner_kmh: f64 = if text.contains("arrive") {
            0.
        } else if text.contains("roundabout") || angle > 100. {
            20.
        } else if angle > 55. {
            27.
        } else if angle > 30. || text.contains("turn left") || text.contains("turn right") {
            38.
        } else {
            continue;
        };
        let corner_kmh = if corner_kmh > 0. {
            corner_kmh * driver.corner_variation(index)
        } else {
            0.
        };
        // Start braking only when the remaining distance requires it.
        let approach =
            ((corner_kmh / 3.6).powi(2) + 2. * driver.braking * (remaining - 12.).max(0.)).sqrt()
                * 3.6;
        cap = cap.min(approach);
    }
    cap
}
fn bend_speed_cap(route: &Route, progress: f64, driver: DriverProfile) -> f64 {
    // Measure heading across 24 m chords to suppress small polyline zigzags.
    // The lateral-acceleration cap is applied ahead with a braking envelope.
    let mut cap = f64::INFINITY;
    let end = (progress + 320.).min(route.length() - 36.);
    let mut at = progress + 24.;
    while at <= end {
        let before = route.position(at - 12.).0;
        let center = route.position(at + 12.).0;
        let after = route.position(at + 36.).0;
        let first = before.bearing(center);
        let second = center.bearing(after);
        let angle = crate::navigation::turn_delta(first, second)
            .abs()
            .to_radians();
        if angle > 5_f64.to_radians() {
            let curvature = angle / 24.;
            let lateral = (1.65 * driver.corner_factor).clamp(1.2, 2.2);
            let corner_mps = (lateral / curvature).sqrt().max(4.);
            let remaining = (at - progress - 18.).max(0.);
            let approach_mps = (corner_mps.powi(2) + 2. * driver.braking * remaining).sqrt();
            cap = cap.min(approach_mps * 3.6);
        }
        at += 24.;
    }
    cap
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Explore,
    Search,
    Nearby,
    Route,
    Saved,
    Preview,
    Settings,
}
#[derive(Clone, Copy)]
struct Positioned {
    handle: HWND,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}
impl Positioned {
    fn new(handle: HWND, x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            handle,
            x,
            y,
            width,
            height,
        }
    }
}
#[derive(PartialEq, Eq)]
enum BrowseMode {
    Search,
    Categories,
    Places,
    AllPlaces,
    Explore,
    Preview,
    Settings,
}
fn page_bounds(total: usize, requested: usize) -> (usize, usize, usize, usize) {
    let pages = total.div_ceil(PAGE_SIZE).max(1);
    let page = requested.min(pages - 1);
    let start = page * PAGE_SIZE;
    (page, start, (start + PAGE_SIZE).min(total), pages)
}
fn page_selection_index(page: usize, local: isize, total: usize) -> Option<usize> {
    if local < 0 || local as usize >= PAGE_SIZE {
        return None;
    }
    let index = page * PAGE_SIZE + local as usize;
    (index < total).then_some(index)
}
fn spoken_places(lines: &[String]) -> String {
    if lines.is_empty() {
        "No nearby places.".into()
    } else {
        format!("{}.", lines.join(". "))
    }
}
fn notable(place: &Place) -> bool {
    place.kind != "address"
        && place.kind != "crossing"
        && (!place.wikipedia.is_empty()
            || !place.wikidata.is_empty()
            || !place.description.is_empty())
        && (place.group == "Sights"
            || place.group == "Outdoors"
            || matches!(
                place.kind.as_str(),
                "library"
                    | "museum"
                    | "theatre"
                    | "university"
                    | "monument"
                    | "memorial"
                    | "marketplace"
                    | "park"
            ))
}
fn drive_poi_candidate(
    area: &Area,
    route: &Route,
    progress: f64,
    speed_kmh: f64,
    mentioned: &HashSet<String>,
) -> Option<usize> {
    let lookahead = (speed_kmh / 3.6 * 7.).clamp(75., 240.);
    area.places
        .iter()
        .enumerate()
        .filter(|(_, place)| {
            !place.name.is_empty()
                && place.kind != "address"
                && place.kind != "crossing"
                && !place.name.eq_ignore_ascii_case(&place.kind)
                && !mentioned.contains(&place.key())
        })
        .filter_map(|(i, place)| {
            let (at, offset) = route.locate_ahead(place.point, progress, lookahead);
            (at >= progress && at <= progress + lookahead && offset <= 75.)
                .then_some((i, at, offset))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1).then(a.2.total_cmp(&b.2)))
        .map(|(i, _, _)| i)
}
fn drive_maneuver_callout(
    route: &Route,
    progress: f64,
    speed_kmh: f64,
    index: usize,
    previewed: bool,
) -> Option<(String, usize, bool)> {
    let maneuver = route.maneuvers.get(index)?;
    if maneuver.at >= route.length() - 3. || maneuver.text.starts_with("Arrive") {
        return None;
    }
    let remaining = maneuver.at - progress;
    if remaining <= 6. {
        let now = if let Some(rest) = maneuver.text.strip_prefix("Turn ") {
            format!("Turning {rest}")
        } else if let Some(rest) = maneuver.text.strip_prefix("Continue ") {
            format!("Continuing {rest}")
        } else {
            maneuver.text.clone()
        };
        return Some((format!("{now}."), index + 1, false));
    }
    let warning = (speed_kmh / 3.6 * 12.).clamp(80., 500.);
    if !previewed && remaining <= warning {
        let metres = (remaining / 10.).round() * 10.;
        let mut instruction = maneuver.text.clone();
        if let Some(first) = instruction.get_mut(0..1) {
            first.make_ascii_lowercase();
        }
        return Some((
            format!("In {metres:.0} metres, {instruction}."),
            index,
            true,
        ));
    }
    None
}
fn drive_address_phrase(area: &Area, point: Point, address: &AddressContext) -> Option<String> {
    let local = area
        .nearby(point)
        .into_iter()
        .find(|i| area.places[*i].kind == "address" && point.distance(area.places[*i].point) <= 40.)
        .map(|i| area.places[i].name.clone());
    let remote = address
        .address_point
        .filter(|p| point.distance(*p) <= 40.)
        .filter(|_| !address.house.is_empty() && !address.street.is_empty())
        .map(|_| format!("{} {}", address.house, address.street));
    let location = if let Some(numbered) = local.or(remote) {
        format!("Near {numbered}")
    } else if area.contains(point) {
        area.location_brief(point)
    } else {
        return None;
    };
    let city = if !address.city.is_empty() && !location.contains(&address.city) {
        format!(", {}", address.city)
    } else {
        String::new()
    };
    Some(format!("{location}{city}."))
}
fn all_poi_choices(area: &Area, position: Point, heading: f64) -> Vec<SearchResult> {
    area.nearby(position)
        .into_iter()
        .filter(|&i| area.places[i].kind != "address")
        .map(|i| SearchResult {
            name: area.describe_place(i, position, heading),
            point: area.places[i].point,
        })
        .collect()
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
unsafe fn text(hwnd: HWND, s: &str) {
    SetWindowTextW(
        hwnd,
        wide(&s.replace("\r\n", "\n").replace('\n', "\r\n")).as_ptr(),
    );
}
#[expect(
    clippy::too_many_arguments,
    reason = "Win32 control creation uses the native position and size arguments"
)]
unsafe fn control(
    parent: HWND,
    class: &str,
    label: &str,
    style: u32,
    id: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) -> HWND {
    let handle = CreateWindowExW(
        0,
        wide(class).as_ptr(),
        wide(label).as_ptr(),
        WS_CHILD | WS_VISIBLE | style,
        x,
        y,
        w,
        h,
        parent,
        id as usize as HMENU,
        GetModuleHandleW(null_mut()),
        null_mut(),
    );
    SendMessageW(
        handle,
        WM_SETFONT,
        GetStockObject(DEFAULT_GUI_FONT) as usize,
        1,
    );
    handle
}
impl App {
    fn request_pc_location(&mut self) {
        match crate::location::request() {
            Ok(access) => {
                let sender = self.location_tx.clone();
                let origin = self.point;
                self.announce("Getting this PC's location from Windows. You can keep exploring while it finishes.");
                std::thread::spawn(move || {
                    let _ = unsafe {
                        windows::Win32::System::WinRT::RoInitialize(
                            windows::Win32::System::WinRT::RO_INIT_MULTITHREADED,
                        )
                    };
                    let _ = sender.send(Reply::PcLocation(crate::location::finish(access), origin));
                });
            }
            Err(e) => self.announce(&e),
        }
    }
    fn set_pc_location_preference(&mut self, enabled: bool) {
        self.start_at_pc_location = enabled;
        let _ = data::save(
            "preferences.json",
            &Preferences {
                start_at_pc_location: enabled,
            },
        );
    }
    fn show_preview(&mut self) {
        if self.driving_settings.google_mode {
            self.announce("Street Preview needs road geometry, which Google Maps does not provide to this mode. Free walking, places, addresses, and routes remain available.");
            return;
        }
        self.preview_options = self.area.preview_roads(self.point);
        if self.preview_options.is_empty() {
            self.show_view(View::Explore);
            unsafe { SetFocus(self.walk) };
            self.announce("No road decision point within 120 metres. Move closer to a mapped street or press S to snap to one.");
            return;
        }
        self.browse_mode = BrowseMode::Preview;
        self.choices = self
            .preview_options
            .iter()
            .map(|road| SearchResult {
                name: format!(
                    "{} toward {}. {:.0} metres to the next junction or road end.",
                    road.name,
                    crate::geo::compass(road.bearing),
                    road.distance
                ),
                point: road.destination,
            })
            .collect();
        self.page = 0;
        self.show_view(View::Preview);
        self.render_results_page();
        unsafe { SetFocus(self.results) };
        let gap = self.point.distance(self.preview_options[0].origin);
        self.announce(&format!("Street preview: {} road choices at the nearest decision point, {:.0} metres away. Up and Down browse; Page Up and Down change pages; Enter moves to the next decision point; Escape returns to free walking.", self.preview_options.len(), gap));
    }
    fn show_saved(&mut self) {
        self.destination_mode = false;
        self.browse_mode = BrowseMode::Search;
        self.choices = data::read("bookmarks.json").unwrap_or_default();
        self.page = 0;
        self.show_view(View::Saved);
        self.render_results_page();
        unsafe { SetFocus(self.results) };
        self.announce(&format!("{} saved places. Up and Down browse across pages; Enter jumps; Ctrl+Enter routes. B saves your current position.", self.choices.len()));
    }
    fn show_route(&mut self) {
        if self.route.is_none() {
            self.open_search(true);
            self.announce("Search for a route destination.");
            return;
        }
        self.route_page = 0;
        self.show_view(View::Route);
        self.render_route_page();
        unsafe { SetFocus(self.route_list) };
    }
    fn open_search(&mut self, destination: bool) {
        self.destination_mode = destination;
        self.browse_mode = BrowseMode::Search;
        self.choices.clear();
        self.page = 0;
        self.show_view(View::Search);
        self.render_results_page();
        unsafe {
            SetFocus(self.search);
            SendMessageW(self.search, EM_SETSEL, 0, -1);
        }
    }
    fn selected_choice_index(&self) -> Option<usize> {
        let local = unsafe { SendMessageW(self.results, LB_GETCURSEL, 0, 0) };
        let total = match self.browse_mode {
            BrowseMode::Categories => self.categories.len(),
            BrowseMode::Settings => self.driving_settings.labels().len(),
            _ => self.choices.len(),
        };
        page_selection_index(self.page, local, total)
    }
    fn render_results_page(&mut self) {
        let settings_labels = self.driving_settings.labels();
        let total = match self.browse_mode {
            BrowseMode::Categories => self.categories.len(),
            BrowseMode::Settings => settings_labels.len(),
            _ => self.choices.len(),
        };
        let (page, start, end, pages) = page_bounds(total, self.page);
        self.page = page;
        let title = match self.browse_mode {
            BrowseMode::Categories => "Categories",
            BrowseMode::Places => "Nearby places",
            BrowseMode::AllPlaces => "All POIs",
            BrowseMode::Explore => "Directional places",
            BrowseMode::Preview => "Road choices",
            BrowseMode::Settings => "Map and driving options",
            BrowseMode::Search if self.view == View::Saved => "Saved places",
            BrowseMode::Search => "Search results",
        };
        unsafe {
            text(
                self.results_controls[0].handle,
                &format!(
                    "{title}: {}–{} of {total}, page {} of {pages}",
                    if total == 0 { 0 } else { start + 1 },
                    end,
                    self.page + 1
                ),
            );
            SendMessageW(self.results, LB_RESETCONTENT, 0, 0);
            #[expect(
                clippy::needless_range_loop,
                reason = "the index selects from one of three lists depending on browse mode"
            )]
            for i in start..end {
                let label = if self.browse_mode == BrowseMode::Settings {
                    settings_labels[i].clone()
                } else if self.browse_mode == BrowseMode::Categories {
                    let category = &self.categories[i];
                    let count = self
                        .area
                        .places
                        .iter()
                        .filter(|p| category == "All places" || p.category() == category)
                        .count();
                    format!("{category} ({count} places)")
                } else {
                    self.choices[i].name.clone()
                };
                SendMessageW(
                    self.results,
                    LB_ADDSTRING,
                    0,
                    wide(&label).as_ptr() as isize,
                );
            }
            if end > start {
                SendMessageW(self.results, LB_SETCURSEL, 0, 0);
            }
            EnableWindow(self.page_previous, (self.page > 0) as i32);
            EnableWindow(self.page_next, (self.page + 1 < pages) as i32);
        }
    }
    fn render_route_page(&mut self) {
        let Some(guidance) = &self.route else { return };
        let total = guidance.route.maneuvers.len();
        let (page, start, end, pages) = page_bounds(total, self.route_page);
        self.route_page = page;
        unsafe {
            text(
                self.route_title,
                &format!(
                    "Route to {}: {:.0} metres, {} instructions; page {} of {}",
                    guidance.route.name,
                    guidance.route.length(),
                    total,
                    self.route_page + 1,
                    pages
                ),
            );
            SendMessageW(self.route_list, LB_RESETCONTENT, 0, 0);
            for i in start..end {
                let label = format!("{}. {}", i + 1, guidance.route.maneuvers[i].text);
                SendMessageW(
                    self.route_list,
                    LB_ADDSTRING,
                    0,
                    wide(&label).as_ptr() as isize,
                );
            }
            if end > start {
                SendMessageW(self.route_list, LB_SETCURSEL, 0, 0);
            }
            EnableWindow(self.page_previous, (self.route_page > 0) as i32);
            EnableWindow(self.page_next, (self.route_page + 1 < pages) as i32);
        }
    }
    fn change_page(&mut self, delta: isize, last_item: bool) {
        let route = self.view == View::Route;
        let total = if route {
            self.route.as_ref().map_or(0, |g| g.route.maneuvers.len())
        } else if self.browse_mode == BrowseMode::Settings {
            self.driving_settings.labels().len()
        } else if self.browse_mode == BrowseMode::Categories {
            self.categories.len()
        } else {
            self.choices.len()
        };
        let current = if route { self.route_page } else { self.page };
        let pages = page_bounds(total, current).3;
        let next = current.saturating_add_signed(delta).min(pages - 1);
        if next == current {
            return;
        }
        if route {
            self.route_page = next;
            self.render_route_page();
        } else {
            self.page = next;
            self.render_results_page();
        }
        let list = if route { self.route_list } else { self.results };
        unsafe {
            if last_item {
                let count = SendMessageW(list, LB_GETCOUNT, 0, 0);
                if count > 0 {
                    SendMessageW(list, LB_SETCURSEL, (count - 1) as usize, 0);
                }
            }
            SetFocus(list);
        }
        self.announce(&format!(
            "Page {} of {}. {} to {} of {}.",
            next + 1,
            pages,
            next * PAGE_SIZE + 1,
            ((next + 1) * PAGE_SIZE).min(total),
            total
        ));
    }
    fn show_view(&mut self, view: View) {
        self.view = view;
        unsafe {
            text(
                self.view_title,
                match view {
                    View::Explore => "Explore",
                    View::Search => "Search",
                    View::Nearby => "Nearby places",
                    View::Route => "Active route",
                    View::Saved => "Saved places",
                    View::Preview => "Street preview",
                    View::Settings => "Map and driving options",
                },
            );
            for control in &self.search_controls {
                ShowWindow(
                    control.handle,
                    if view == View::Search {
                        SW_SHOW
                    } else {
                        SW_HIDE
                    },
                );
            }
            for control in &self.results_controls {
                ShowWindow(
                    control.handle,
                    if (view != View::Settings
                        || control.handle == self.results
                        || control.handle == self.results_controls[0].handle)
                        && matches!(
                            view,
                            View::Search
                                | View::Nearby
                                | View::Saved
                                | View::Preview
                                | View::Settings
                        )
                    {
                        SW_SHOW
                    } else {
                        SW_HIDE
                    },
                );
                let offset = if matches!(
                    view,
                    View::Nearby | View::Saved | View::Preview | View::Settings
                ) {
                    -58
                } else {
                    0
                };
                MoveWindow(
                    control.handle,
                    control.x,
                    control.y + offset,
                    control.width,
                    control.height,
                    1,
                );
            }
            let page_y = if view == View::Search { 144 } else { 86 };
            for (handle, x, width) in [(self.page_previous, 540, 110), (self.page_next, 660, 115)] {
                MoveWindow(handle, x, page_y, width, 24, 1);
                ShowWindow(
                    handle,
                    if matches!(view, View::Explore | View::Settings) {
                        SW_HIDE
                    } else {
                        SW_SHOW
                    },
                );
            }
            for control in &self.route_controls {
                ShowWindow(
                    control.handle,
                    if view == View::Route {
                        SW_SHOW
                    } else {
                        SW_HIDE
                    },
                );
            }
            for control in &self.settings_controls {
                ShowWindow(
                    control.handle,
                    if view == View::Settings {
                        SW_SHOW
                    } else {
                        SW_HIDE
                    },
                );
            }
            let offset = match view {
                View::Explore => -176,
                View::Nearby | View::Saved | View::Preview | View::Settings => -55,
                View::Search | View::Route => 0,
            };
            for control in &self.walking_controls {
                let height = if control.handle == self.output {
                    match view {
                        View::Explore => 364,
                        View::Nearby | View::Saved | View::Preview | View::Settings => 243,
                        View::Search | View::Route => 188,
                    }
                } else {
                    control.height
                };
                MoveWindow(
                    control.handle,
                    control.x,
                    control.y + offset,
                    control.width,
                    height,
                    1,
                );
            }
        }
    }
    fn explore(&mut self, ahead: bool) {
        self.show_view(View::Nearby);
        let indices = self
            .area
            .exploration_places(self.point, self.heading, ahead);
        self.choices = indices
            .iter()
            .map(|&i| SearchResult {
                name: self.area.describe_place(i, self.point, self.heading),
                point: self.area.places[i].point,
            })
            .collect();
        self.browse_mode = BrowseMode::Explore;
        self.page = 0;
        self.render_results_page();
        if !self.choices.is_empty() {
            unsafe { SetFocus(self.results) };
        }
        let mode = if ahead { "Ahead of me" } else { "Around me" };
        if self.choices.is_empty() {
            self.announce(&format!("{mode}: no named places in the downloaded map. Try Nearby for all mapped features."));
        } else {
            self.announce(&format!(
                "{mode}: {} places. Use Up and Down to explore across pages; Enter jumps; Ctrl+Enter routes. Backspace returns to walking.",
                self.choices.len()
            ));
        }
    }
    fn show_all_places(&mut self) {
        self.browse_mode = BrowseMode::AllPlaces;
        self.choices = all_poi_choices(&self.area, self.point, self.heading);
        self.page = 0;
        self.show_view(View::Nearby);
        self.render_results_page();
        unsafe { SetFocus(self.results) };
        self.announce(&format!("All POIs: {} places, nearest first. Up and Down browse across pages; Enter jumps; Ctrl+Enter or G routes. Backspace returns to walking.", self.choices.len()));
    }
    fn walking_step(&mut self) {
        if !self.muted
            && self
                .last_walk
                .is_none_or(|last| last.elapsed() >= Duration::from_secs(5))
            && unsafe { GetForegroundWindow() == self.hwnd }
        {
            self.audio.movement();
        }
        self.last_walk = Some(Instant::now());
    }
    fn cue_place(&mut self, index: usize) {
        self.cue_point(self.area.places[index].point);
    }
    fn cue_point(&mut self, point: Point) {
        if self
            .area
            .places
            .iter()
            .any(|place| place.point == point && place.kind == "crossing")
        {
            self.audio.safety(point, self.point, self.heading);
        } else {
            self.audio.cue(point, self.point, self.heading);
        }
    }
    fn help(&self) {
        let documentation_dir = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(std::path::Path::to_path_buf));
        let markdown = documentation_dir
            .as_ref()
            .and_then(|dir| std::fs::read_to_string(dir.join("README.md")).ok())
            .unwrap_or_else(|| include_str!("../README.md").to_owned());
        let base = documentation_dir
            .and_then(|dir| reqwest::Url::from_directory_path(dir).ok())
            .map(|url| url.to_string().replace('&', "&amp;"))
            .unwrap_or_default();
        let mut body = String::new();
        pulldown_cmark::html::push_html(
            &mut body,
            pulldown_cmark::Parser::new_ext(&markdown, pulldown_cmark::Options::all()),
        );
        let page = format!("<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><base href=\"{base}\"><title>Streetwalk guide</title><style>body{{font:1.1rem/1.5 system-ui,sans-serif;max-width:75ch;margin:2rem auto;padding:0 1rem}}table{{border-collapse:collapse}}td,th{{border:1px solid;padding:.4rem}}code{{font-size:.95em}}</style><main>{body}</main></html>");
        let path = data::directory().join("README.html");
        let result =
            std::fs::create_dir_all(data::directory()).and_then(|_| std::fs::write(&path, page));
        if let Err(e) = result {
            self.announce(&format!("Could not open README: {e}"));
            return;
        }
        unsafe {
            let path = wide(&path.to_string_lossy());
            let outcome = ShellExecuteW(
                self.hwnd,
                wide("open").as_ptr(),
                path.as_ptr(),
                null_mut(),
                null_mut(),
                SW_SHOWNORMAL,
            );
            if outcome as isize <= 32 {
                self.announce("Could not open the README in your default browser.");
            }
        }
    }
    fn where_am_i(&self) {
        if self.area.is_demo() {
            self.announce(&self.area.full_location(
                self.point,
                self.heading,
                &AddressContext::default(),
            ));
        } else if self
            .tx
            .send(Job::WhereAmI(self.point, self.heading))
            .is_ok()
        {
            self.announce("Looking up your full address.");
        } else {
            self.announce(&self.area.full_location(
                self.point,
                self.heading,
                &AddressContext::default(),
            ));
        }
    }
    fn announce(&self, s: &str) {
        self.append_transcript(s);
        unsafe {
            if !self.muted && GetForegroundWindow() == self.hwnd {
                self.speech.say(s);
            }
        }
    }
    fn update_attribution(&self) {
        unsafe {
            if self.driving_settings.google_mode {
                text(self.map_attribution, "Map and places: Google Maps");
                text(
                    self.route_attribution,
                    "Routes and addresses: Google Maps | Weather: Open-Meteo",
                );
            } else {
                text(
                    self.map_attribution,
                    "Map data © OpenStreetMap contributors · ODbL · openstreetmap.org/copyright",
                );
                text(self.route_attribution, "Routes: FOSSGIS / OSRM | Weather: Open-Meteo | Fix map: openstreetmap.org/fixthemap");
            }
        }
    }
    fn append_transcript(&self, s: &str) {
        let mut entries = self.announcements.borrow_mut();
        entries.push_back(s.to_owned());
        if entries.len() > 40 {
            entries.pop_front();
        }
        unsafe {
            text(
                self.output,
                &entries.iter().cloned().collect::<Vec<_>>().join("\r\n\r\n"),
            );
            SendMessageW(self.output, EM_SETSEL, -1isize as usize, -1);
        }
    }
    fn persist(&self) {
        // Google route and place results stay in memory. Only the virtual position is restored.
        let google = self.driving_settings.google_mode;
        let _ = data::save(
            "session.json",
            &Session {
                area: if google {
                    Area::google_empty(self.point)
                } else {
                    self.area.clone()
                },
                point: self.point,
                heading: self.heading,
                step: self.step,
                turn: self.turn,
                route: if google {
                    None
                } else {
                    self.route.as_ref().map(|g| g.route.clone())
                },
            },
        );
    }
    fn describe(&self) {
        let mut location = if self.area.contains(self.point) {
            format!(
                "{}, facing {}.",
                self.area.location_brief(self.point),
                crate::geo::compass(self.heading)
            )
        } else {
            format!(
                "Map data not yet available here. Heading {} at {:.0} degrees.",
                crate::geo::compass(self.heading),
                self.heading
            )
        };
        if let Some(drive) = &self.drive {
            location.push_str(&format!(
                " Virtual drive {} at {:.0} kilometres per hour, {}.",
                if drive.paused { "paused" } else { "moving" },
                drive.actual_speed_kmh,
                if drive.tagged_speed {
                    "mapped speed limit"
                } else {
                    "estimated road speed"
                }
            ));
        }
        self.announce(&location);
        // Keep settings visible without repeating them in every walking announcement.
        self.append_transcript(&format!(
            "{} | Step: {} metres | {} | Heading {:.0} degrees; turn {:.0} degrees. {}",
            self.area.name,
            [1, 5, 10, 25, 50, 100][self.step],
            if self.muted {
                "Speech off"
            } else {
                "Speech on"
            },
            self.heading,
            self.turn,
            self.route.as_ref().map(|g| g.next()).unwrap_or_default()
        ));
    }
    fn move_to(&mut self, p: Point, heading: f64) {
        if !p.valid() {
            self.announce("This position is outside the supported latitude range.");
            return;
        }
        if self.area.is_demo() && !self.area.contains(p) {
            self.announce("Edge of the fictional demo. Ctrl+F loads a real place with automatic map downloads.");
            return;
        }
        if self.drive.take().is_some() {
            self.announce("Virtual drive ended. Exploring from this position.");
        }
        self.history.push((self.point, self.heading));
        if self.history.len() > 1000 {
            self.history.remove(0);
        }
        self.point = p;
        self.heading = heading.rem_euclid(360.);
        self.audio.update(self.point, self.heading, unsafe {
            GetForegroundWindow() == self.hwnd
        });
        if !self.area.contains(p) {
            if let Some(i) = self.spare.iter().position(|a| a.contains(p)) {
                let a = self.spare.remove(i);
                self.swap_area(a);
            }
        }
        unsafe {
            text(self.heading_input, &format!("{:.0}", self.heading));
        }
        self.passive_update();
        self.ensure_coverage(false);
        if self.driving_settings.google_mode
            && self.drive.is_none()
            && !self.google_walk_pending
            && self.last_google_walk_request.is_none_or(|(last, at)| {
                self.point.distance(last) >= 100. && at.elapsed() >= Duration::from_secs(20)
            })
        {
            self.last_google_walk_request = Some((self.point, Instant::now()));
            self.google_walk_pending = self
                .tx
                .send(Job::GoogleWalkAddress(self.point, self.map_generation))
                .is_ok();
        }
    }
    fn swap_area(&mut self, a: Area) {
        let a = a.normalize_legacy();
        let old = std::mem::replace(&mut self.area, a);
        self.spare.push(old);
        if self.spare.len() > 8 {
            self.spare.remove(0);
        }
    }
    fn passive_update(&mut self) {
        let events = if self.area.contains(self.point) {
            self.announcer.update(&self.area, self.point, self.heading)
        } else {
            vec![]
        };
        let instruction = self.route.as_mut().and_then(|g| g.update(self.point));
        if let Some(instruction) = instruction {
            self.announce(&instruction);
        } else if !events.is_empty() {
            if !self.muted && unsafe { GetForegroundWindow() == self.hwnd } {
                let nearby = self.area.nearby(self.point);
                if let Some(i) = events.iter().find_map(|event| {
                    nearby
                        .iter()
                        .copied()
                        .find(|i| event == &self.area.callout_place(*i, self.point, self.heading))
                }) {
                    self.cue_place(i);
                }
            }
            self.announce(&events.join(". "));
        }
    }
    fn ensure_coverage(&mut self, force: bool) {
        if self.area.is_demo() || self.cover_pending {
            return;
        }
        let threshold = if self.driving_settings.google_mode && self.drive.is_some() {
            self.area.radius - 500.
        } else if self.driving_settings.google_mode {
            550.
        } else {
            self.area.radius - 450.
        };
        if !force && self.area.center.distance(self.point) < threshold {
            return;
        }
        let delay = if self.map_error { 60 } else { 10 };
        if !force
            && self
                .last_cover
                .is_some_and(|t| t.elapsed() < Duration::from_secs(delay))
        {
            return;
        }
        let target = if self.area.contains(self.point) {
            self.point.walk(self.heading, 350.)
        } else {
            self.point
        };
        if !target.valid() {
            return;
        }
        if self
            .map_tx
            .send(Job::Cover(target, self.map_generation))
            .is_ok()
        {
            self.cover_pending = true;
            self.last_cover = Some(Instant::now());
        }
    }
    fn face(&mut self) {
        if let Some(value) = self.number(self.heading_input, 0., 360.) {
            self.heading = value.rem_euclid(360.);
            self.audio.update(self.point, self.heading, unsafe {
                GetForegroundWindow() == self.hwnd
            });
            self.announce(&format!(
                "Facing {} at {:.0} degrees",
                crate::geo::compass(self.heading),
                self.heading
            ));
            unsafe {
                SetFocus(self.walk);
            }
        }
    }
    fn set_turn(&mut self) {
        if let Some(value) = self.number(self.turn_input, 1., 180.) {
            self.turn = value;
            self.announce(&format!("Turn increment {:.0} degrees", value));
            unsafe {
                SetFocus(self.walk);
            }
        }
    }
    fn number(&self, hwnd: HWND, min: f64, max: f64) -> Option<f64> {
        unsafe {
            let mut v = [0u16; 64];
            let n = GetWindowTextW(hwnd, v.as_mut_ptr(), 64);
            let parsed = String::from_utf16_lossy(&v[..n as usize])
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|x| x.is_finite() && x.fract() == 0. && *x >= min && *x <= max);
            if parsed.is_none() {
                self.announce(&format!("Enter a whole number from {min} to {max}."));
            }
            parsed
        }
    }
    fn plan(&mut self, destination: SearchResult) {
        if self.busy {
            self.announce("A request is already running. Try again when it finishes.");
            return;
        }
        self.route_generation += 1;
        if self.area.is_demo() {
            match Route::demo(&self.area, self.point, destination.name, destination.point) {
                Ok(r) => self.activate_route(r),
                Err(e) => self.announce(&e),
            };
            return;
        }
        self.start(Job::Route(self.point, destination, self.route_generation));
    }
    fn activate_route(&mut self, route: Route) {
        self.drive = None;
        self.audio.stop();
        let first = route
            .maneuvers
            .first()
            .map(|m| m.text.clone())
            .unwrap_or_default();
        let join = self.point.distance(route.points[0]);
        let message=format!("Walking route to {}: {:.0} metres. Route starts {join:.0} metres away. {first}. F advances along it; W walks in your chosen heading; T reads the next instruction.",route.name,route.length());
        let mut guidance = Guidance::new(route);
        guidance.progress = guidance.route.locate(self.point, 0.).0;
        self.route = Some(guidance);
        self.show_route();
        self.announce(&message);
        self.persist();
    }
    fn route_selected(&mut self) {
        if self.browse_mode == BrowseMode::Categories {
            self.select_category();
            return;
        }
        if let Some(i) = self.selected_choice_index() {
            if self.browse_mode == BrowseMode::Preview {
                if let Some(road) = self.preview_options.get(i) {
                    self.plan(SearchResult {
                        name: format!("{} road end", road.name),
                        point: road.destination,
                    });
                    return;
                }
            }
            if let Some(d) = self.choices.get(i).cloned() {
                self.plan(d);
                return;
            }
        }
        self.announce("Select a destination first. Ctrl+D searches for a destination.");
    }
    fn selected_place(&self) -> Option<Place> {
        let choice = self
            .selected_choice_index()
            .and_then(|i| self.choices.get(i))?;
        let point = choice.point;
        self.area
            .places
            .iter()
            .filter(|p| p.point.distance(point) < 12.)
            .min_by(|a, b| {
                let rank = |p: &Place| (!choice.name.starts_with(&p.name), p.point.distance(point));
                let a = rank(a);
                let b = rank(b);
                a.0.cmp(&b.0).then_with(|| a.1.total_cmp(&b.1))
            })
            .cloned()
    }
    fn place_info(&mut self) {
        let place = self.selected_place().or_else(|| {
            if self.view != View::Explore {
                return None;
            }
            self.area
                .nearby(self.point)
                .into_iter()
                .find(|i| {
                    notable(&self.area.places[*i])
                        && self.point.distance(self.area.places[*i].point) <= 300.
                })
                .map(|i| self.area.places[i].clone())
        });
        let Some(place) = place else {
            self.announce("No described place is selected or nearby.");
            return;
        };
        if self.busy {
            self.announce("A request is already running. Try again when it finishes.");
            return;
        }
        self.start(Job::PlaceContext(place, false));
    }
    fn drive_selected(&mut self) {
        let Some(destination) = self
            .selected_choice_index()
            .and_then(|i| self.choices.get(i))
            .cloned()
        else {
            self.announce("Select a destination, then choose Drive tour.");
            return;
        };
        if self.area.is_demo() {
            self.announce("Virtual drives need a real mapped place. Search for a town first.");
            return;
        }
        if self.busy {
            self.announce("A request is already running. Try again when it finishes.");
            return;
        }
        self.drive_generation += 1;
        self.start(Job::DriveRoute(
            self.point,
            destination,
            self.driving_settings.route_preference,
            self.drive_generation,
        ));
    }
    fn show_settings(&mut self) {
        self.browse_mode = BrowseMode::Settings;
        self.page = 0;
        self.show_view(View::Settings);
        self.render_results_page();
        unsafe { SetFocus(self.results) };
        self.announce("Map and driving options. Choose a setting, then press Enter or Right to increase it, or Left to decrease it. Escape returns to Explore.");
    }
    fn adjust_settings(&mut self, direction: i32) {
        let Some(index) = self.selected_choice_index() else {
            return;
        };
        if index == 7 {
            if self.busy {
                self.announce("Wait for the current request before changing map sources.");
                return;
            }
            if !self.driving_settings.google_mode {
                if let Err(error) = crate::google::key() {
                    self.announce(&error);
                    return;
                }
            }
        }
        self.driving_settings.adjust(index, direction);
        if index == 2 {
            self.traffic_generation = self.traffic_generation.wrapping_add(1);
            self.traffic_pending = false;
            self.incidents_pending = false;
            self.last_traffic_request = None;
            self.last_incidents_request = None;
            if !self.driving_settings.live_traffic {
                self.last_traffic = None;
                self.incidents.clear();
                if let Some(drive) = &mut self.drive {
                    drive.clear_live_traffic();
                }
            }
        }
        let label = self.driving_settings.labels()[index].clone();
        self.render_results_page();
        unsafe {
            SendMessageW(self.results, LB_SETCURSEL, index % PAGE_SIZE, 0);
            SetFocus(self.results);
        }
        if let Err(e) = data::save("driving_settings.json", &self.driving_settings) {
            self.announce(&format!("Could not save driving options: {e}"));
        }
        self.announce(&label);
        if index == 7 {
            let current = self.point;
            self.route = None;
            self.drive = None;
            self.spare.clear();
            self.history.clear();
            self.area = if self.driving_settings.google_mode {
                Area::google_empty(current)
            } else {
                Area::demo()
            };
            self.announcer.reset(&self.area, current);
            self.update_attribution();
            let _ = self
                .tx
                .send(Job::SetGoogleMode(self.driving_settings.google_mode));
            let _ = self
                .map_tx
                .send(Job::SetGoogleMode(self.driving_settings.google_mode));
            self.start(Job::Load(SearchResult {
                name: "Current position".into(),
                point: current,
            }));
        }
    }
    fn request_weather(&mut self, spoken: bool) {
        if self.weather_pending {
            if spoken {
                self.announce("Weather update is already on its way.");
            }
            return;
        }
        if !spoken
            && self.last_weather_request.is_some_and(|(point, at)| {
                self.point.distance(point) < 5_000. && at.elapsed() < Duration::from_secs(600)
            })
        {
            return;
        }
        if spoken
            && self.last_weather_request.is_some_and(|(point, at)| {
                self.point.distance(point) < 1_000. && at.elapsed() < Duration::from_secs(120)
            })
        {
            if let Some(weather) = &self.last_weather {
                self.announce(&format!("Current modeled weather: {}.", weather.summary()));
                self.append_transcript(
                    "Source: Open-Meteo, https://open-meteo.com/. Weather data CC BY 4.0.",
                );
                return;
            }
        }
        let point = self.point;
        self.last_weather_request = Some((point, Instant::now()));
        self.weather_pending = self.tx.send(Job::Weather(point, spoken)).is_ok();
        if spoken && self.drive.is_none() {
            self.announce(if self.weather_pending {
                "Checking current weather at your virtual position."
            } else {
                "Weather service is unavailable."
            });
        }
    }
    fn request_traffic(&mut self, force: bool) {
        if !self.driving_settings.live_traffic || self.traffic_pending || self.drive.is_none() {
            return;
        }
        if !force
            && self.last_traffic_request.is_some_and(|(point, at)| {
                self.point.distance(point) < 1_500. && at.elapsed() < Duration::from_secs(120)
            })
        {
            return;
        }
        self.last_traffic_request = Some((self.point, Instant::now()));
        self.traffic_pending = self
            .tx
            .send(Job::Traffic(
                self.point,
                self.heading,
                self.traffic_generation,
            ))
            .is_ok();
    }
    fn request_incidents(&mut self, force: bool) {
        if !self.driving_settings.live_traffic || self.incidents_pending || self.drive.is_none() {
            return;
        }
        if !force
            && self.last_incidents_request.is_some_and(|(point, at)| {
                self.point.distance(point) < 5_000. && at.elapsed() < Duration::from_secs(300)
            })
        {
            return;
        }
        self.last_incidents_request = Some((self.point, Instant::now()));
        self.incidents_pending = self
            .tx
            .send(Job::Incidents(self.point, self.traffic_generation))
            .is_ok();
    }
    fn request_drive_address(&mut self, force: bool) {
        if self.drive_address_pending || self.drive.is_none() {
            return;
        }
        if !force
            && self.last_drive_address_request.is_some_and(|(point, at)| {
                self.point.distance(point) < 1_000. && at.elapsed() < Duration::from_secs(75)
            })
        {
            return;
        }
        self.last_drive_address_request = Some((self.point, Instant::now()));
        self.drive_address_pending = self
            .tx
            .send(Job::DriveAddress(self.point, self.drive_generation))
            .is_ok();
    }
    fn request_elevation(&mut self) {
        let Some(drive) = &mut self.drive else { return };
        if drive.elevation_pending
            || drive.progress + 4_000. < drive.elevation_until
            || drive.progress + 150. >= drive.route.length()
        {
            return;
        }
        let start = (drive.progress / 100.).floor() * 100.;
        let end = (start + 8_000.).min(drive.route.length());
        let mut at = start;
        let mut samples = Vec::new();
        while at <= end && samples.len() < 99 {
            samples.push((at, drive.route.position(at).0));
            at += 100.;
        }
        if samples.last().is_some_and(|(at, _)| *at < end) {
            samples.push((end, drive.route.position(end).0));
        }
        drive.elevation_pending = self
            .tx
            .send(Job::Elevations(samples, self.drive_generation))
            .is_ok();
    }
    fn drive_tick(&mut self) {
        let Some(drive) = &mut self.drive else { return };
        if drive.paused
            || self.view != View::Explore
            || unsafe { GetForegroundWindow() != self.hwnd }
        {
            if self.driving_settings.ev_audio {
                self.audio.ev_update(EvInput {
                    speed_kmh: drive.actual_speed_kmh as f32,
                    road_event_kind: drive.road_event_kind,
                    road_event_token: drive.road_event_counter,
                    active: false,
                    ..EvInput::default()
                });
            }
            drive.last_tick = Instant::now();
            return;
        }
        let now = Instant::now();
        let elapsed = now.duration_since(drive.last_tick).as_secs_f64().min(0.5);
        drive.last_tick = now;
        let mut speed_callout = None;
        let mut traffic_callout = None;
        if now.duration_since(drive.last_speed_check) >= Duration::from_secs(1) {
            drive.last_speed_check = now;
            let (route_point, route_heading) = drive.route.position(drive.progress);
            drive.texture = self.area.driving_texture(route_point);
            if self
                .last_traffic
                .as_ref()
                .is_some_and(|flow| !flow.matches_route(route_point, route_heading))
            {
                drive.traffic_kmh = None;
                drive.lead = None;
                self.last_traffic = None;
                if self
                    .last_traffic_request
                    .is_some_and(|(_, at)| at.elapsed() >= Duration::from_secs(30))
                {
                    self.last_traffic_request = None;
                }
            }
            let (speed, tagged) = if self.driving_settings.google_mode {
                (
                    drive
                        .route
                        .cruise_speed(drive.progress)
                        .unwrap_or(self.driving_settings.fallback.suburban),
                    false,
                )
            } else {
                self.area
                    .driving_speed(route_point, route_heading, self.driving_settings.fallback)
            };
            if (speed - drive.speed_kmh).abs() >= 5. {
                drive.speed_kmh = speed;
                drive.tagged_speed = tagged;
                if tagged && now.duration_since(drive.last_speed_callout) >= Duration::from_secs(12)
                {
                    drive.last_speed_callout = now;
                    speed_callout = Some(speed);
                }
            }
            if self.driving_settings.signal_wait_seconds > 0 && drive.stop.is_none() {
                let braking = (drive.actual_speed_kmh / 3.6).powi(2) / (2. * 1.4)
                    + drive.actual_speed_kmh / 3.6;
                let lookahead = (braking + 80.).clamp(80., 400.);
                let candidate = self
                    .area
                    .signalized_crossings
                    .iter()
                    .filter(|p| {
                        !drive
                            .seen_signals
                            .contains(&format!("{:.6},{:.6}", p.lat, p.lon))
                    })
                    .map(|p| {
                        let (at, offset) = drive.route.locate_ahead(*p, drive.progress, lookahead);
                        (*p, at, offset)
                    })
                    .filter(|(_, at, offset)| *offset <= 12. && *at > drive.progress + 8.)
                    .min_by(|a, b| a.1.total_cmp(&b.1));
                if let Some((p, at, _)) = candidate {
                    drive
                        .seen_signals
                        .insert(format!("{:.6},{:.6}", p.lat, p.lon));
                    drive.stop = Some(TrafficStop {
                        at: (at - 5.).max(drive.progress),
                        remaining: self.driving_settings.signal_wait_seconds as f64,
                        reached: false,
                    });
                }
            }
        }
        let mut target = (drive.speed_kmh * drive.driver.cruise_factor)
            .min(drive.traffic_kmh.unwrap_or(f64::INFINITY))
            .min(bend_speed_cap(&drive.route, drive.progress, drive.driver))
            .min(maneuver_speed_cap(
                &drive.route,
                drive.progress,
                drive.driver,
            ));
        if let (Some(lead), Some(flow_speed)) = (&mut drive.lead, drive.traffic_kmh) {
            target = target.min(lead.advance(
                elapsed,
                drive.progress,
                drive.actual_speed_kmh,
                flow_speed,
                drive.driver.headway_seconds,
            ));
        }
        if let Some(stop) = &mut drive.stop {
            if stop.reached {
                stop.remaining -= elapsed;
                target = 0.;
                if stop.remaining <= 0. {
                    drive.stop = None;
                    drive.departure_delay = drive.driver.reaction_seconds;
                    traffic_callout = Some("Traffic is clear. Continuing.");
                }
            } else {
                let remaining = (stop.at - drive.progress).max(0.);
                target = target.min((2. * 1.4 * (remaining - 6.).max(0.)).sqrt() * 3.6);
            }
        }
        if drive.actual_speed_kmh < 0.1 && target > 0. && drive.departure_delay > 0. {
            drive.departure_delay = (drive.departure_delay - elapsed).max(0.);
            target = 0.;
        }
        let old_speed = drive.actual_speed_kmh;
        let grade = route_grade(&drive.elevation_samples, drive.progress);
        (drive.actual_speed_kmh, drive.actual_acceleration_mps2) = approach_speed_on_grade(
            old_speed,
            target,
            elapsed,
            drive.actual_acceleration_mps2,
            drive.driver,
            grade,
        );
        let distance = elapsed * (old_speed + drive.actual_speed_kmh) / 7.2;
        let road_event_kind = if distance > 0.1 {
            self.area.road_events.iter().find_map(|event| {
                let key = format!(
                    "{:.6},{:.6}:{}",
                    event.point.lat, event.point.lon, event.kind
                );
                if drive.seen_road_events.contains(&key) {
                    return None;
                }
                let (at, offset) =
                    drive
                        .route
                        .locate_ahead(event.point, drive.progress, distance + 8.);
                if offset <= 6. && at >= drive.progress - 2. && at <= drive.progress + distance + 3.
                {
                    drive.seen_road_events.insert(key);
                    Some(if event.kind == "rumble_strip" { 2 } else { 1 })
                } else {
                    None
                }
            })
        } else {
            None
        };
        if let Some(kind) = road_event_kind {
            drive.road_event_counter = drive.road_event_counter.wrapping_add(1);
            drive.road_event_kind = kind;
        }
        if self.driving_settings.ev_audio && !self.muted {
            let heading = drive.route.position(drive.progress).1;
            let weather = self.last_weather.as_ref();
            let traffic_density = estimated_traffic_activity(
                self.area.driving_urbanity(self.point),
                self.last_traffic.as_ref(),
            );
            self.audio.ev_update(EvInput {
                speed_kmh: drive.actual_speed_kmh as f32,
                acceleration_mps2: drive.actual_acceleration_mps2 as f32,
                grade: grade as f32,
                road_texture: drive.texture as f32,
                bumpiness: self.area.driving_bumpiness(self.point) as f32,
                volume: self.driving_settings.ev_volume as f32 / 100.,
                wind_kmh: weather.map_or(0., |w| w.wind_kmh as f32),
                wind_from_deg: weather.map_or(0., |w| (w.wind_from_deg - heading) as f32),
                traffic_density,
                precipitation_mm: weather.map_or(0., |w| w.precipitation_mm as f32),
                road_event_kind: drive.road_event_kind,
                road_event_token: drive.road_event_counter,
                indicator: turn_indicator_side(
                    &drive.route,
                    drive.progress,
                    drive.actual_speed_kmh,
                ),
                active: true,
            });
        } else {
            drive.road_event_counter = 0;
            drive.road_event_kind = 0;
            self.audio.ev_stop();
        }
        drive.progress = (drive.progress + distance).min(drive.route.length());
        if let Some(stop) = &mut drive.stop {
            if !stop.reached && drive.progress >= stop.at - 0.5 {
                drive.progress = stop.at;
                drive.actual_speed_kmh = 0.;
                drive.actual_acceleration_mps2 = 0.;
                stop.reached = true;
                traffic_callout = Some("Waiting at a mapped signalized crossing.");
            }
        }
        let (point, heading) = drive.route.position(drive.progress);
        let arrived = drive.progress >= drive.route.length();
        if let Some(limit) = speed_callout {
            self.announce(&format!(
                "Mapped speed limit: {:.0} kilometres per hour.",
                limit
            ));
        }
        if let Some(message) = traffic_callout {
            self.announce(message);
        }
        self.point = point;
        self.heading = heading;
        if !self.area.contains(point) {
            if let Some(i) = self.spare.iter().position(|a| a.contains(point)) {
                let area = self.spare.remove(i);
                self.swap_area(area);
            }
        }
        self.audio.update(self.point, self.heading, true);
        self.request_weather(false);
        self.request_elevation();
        self.request_traffic(false);
        self.request_incidents(false);
        self.request_drive_address(false);
        let mut guidance_spoken = false;
        if let Some(drive) = &mut self.drive {
            if let Some((callout, next, previewed)) = drive_maneuver_callout(
                &drive.route,
                drive.progress,
                drive.actual_speed_kmh.max(25.),
                drive.next_maneuver,
                drive.maneuver_previewed,
            ) {
                drive.next_maneuver = next;
                drive.maneuver_previewed = previewed;
                drive.last_guidance_announcement = now;
                drive.last_guidance_progress = drive.progress;
                guidance_spoken = true;
                self.announce(&callout);
            }
        }
        if !guidance_spoken {
            if let Some(drive) = &mut self.drive {
                let next = drive.route.maneuvers.get(drive.next_maneuver);
                let distance = next
                    .map(|m| m.at - drive.progress)
                    .unwrap_or_else(|| drive.route.length() - drive.progress);
                if now.duration_since(drive.last_guidance_announcement) >= Duration::from_secs(45)
                    && drive.progress - drive.last_guidance_progress >= 400.
                    && distance >= 600.
                {
                    let road = self
                        .area
                        .context(point)
                        .and_then(|(_, spoken)| spoken.strip_prefix("On ").map(str::to_owned));
                    let action = road
                        .map(|name| format!("Continue on {name}"))
                        .unwrap_or_else(|| "Continue along the route".into());
                    let distance_km = distance / 1_000.;
                    drive.last_guidance_announcement = now;
                    drive.last_guidance_progress = drive.progress;
                    guidance_spoken = true;
                    self.announce(&format!("{action} for about {distance_km:.1} kilometres."));
                }
            }
        }
        if let Some(drive) = &mut self.drive {
            let context = if self.area.contains(point) {
                self.area.context(point)
            } else {
                None
            };
            let key = context.as_ref().map(|(key, _)| key.clone());
            if key != drive.last_street_context {
                drive.last_street_context = key;
                if !guidance_spoken
                    && now.duration_since(drive.last_street_announcement) >= Duration::from_secs(15)
                {
                    if let Some((_, spoken)) = context {
                        drive.last_street_announcement = now;
                        drive.last_street_speech = now;
                        drive.last_guidance_announcement = now;
                        drive.last_guidance_progress = drive.progress;
                        guidance_spoken = true;
                        self.announce(&format!("{spoken}."));
                    }
                }
            }
        }
        if !guidance_spoken
            && self.drive.as_ref().is_some_and(|drive| {
                now.duration_since(drive.last_guidance_announcement) >= Duration::from_secs(5)
            })
        {
            if let Some(address) = self.pending_drive_address.take() {
                if let Some(drive) = &mut self.drive {
                    drive.last_guidance_announcement = now;
                    drive.last_guidance_progress = drive.progress;
                    if address.starts_with("On ") {
                        drive.last_street_speech = now;
                    }
                }
                guidance_spoken = true;
                self.announce(&address);
            }
        }
        if let Some(drive) = &mut self.drive {
            if !guidance_spoken
                && now.duration_since(drive.last_incident_callout) >= Duration::from_secs(15)
            {
                let candidate = self
                    .incidents
                    .iter()
                    .filter(|incident| !drive.mentioned_incidents.contains(&incident.id))
                    .filter_map(|incident| {
                        incident
                            .points
                            .iter()
                            .filter_map(|p| {
                                let (at, offset) =
                                    drive.route.locate_ahead(*p, drive.progress, 2_000.);
                                (offset <= 45.
                                    && at >= drive.progress
                                    && at <= drive.progress + 2_000.)
                                    .then_some(at)
                            })
                            .min_by(|a, b| a.total_cmp(b))
                            .map(|at| (incident, at))
                    })
                    .min_by(|a, b| a.1.total_cmp(&b.1));
                if let Some((incident, at)) = candidate {
                    let description = incident.description.clone();
                    let delay = incident.delay_seconds;
                    let id = incident.id.clone();
                    let distance_km = (at - drive.progress) / 1000.;
                    drive.mentioned_incidents.insert(id);
                    drive.last_incident_callout = now;
                    guidance_spoken = true;
                    let extra = delay
                        .filter(|s| *s >= 60)
                        .map(|s| format!(" Reported delay about {} minutes.", s.div_ceil(60)))
                        .unwrap_or_default();
                    self.announce(&format!(
                        "Traffic report about {:.1} kilometres ahead: {}.{}",
                        distance_km,
                        description.trim_end_matches('.'),
                        extra
                    ));
                    self.append_transcript("Source: TomTom Traffic Incidents API. Reported road events may change or affect a nearby parallel road.");
                }
            }
        }
        if let Some(drive) = &mut self.drive {
            if !guidance_spoken
                && drive_poi_ready_after_street(now, drive.last_street_speech)
                && now.duration_since(drive.last_callout) >= Duration::from_secs(8)
                && now.duration_since(drive.last_poi_scan) >= Duration::from_secs(1)
            {
                drive.last_poi_scan = now;
                let candidate = drive_poi_candidate(
                    &self.area,
                    &drive.route,
                    drive.progress,
                    drive.actual_speed_kmh.max(drive.speed_kmh),
                    &drive.mentioned,
                );
                if let Some(i) = candidate {
                    let place = self.area.places[i].clone();
                    drive.mentioned.insert(place.key());
                    drive.last_callout = now;
                    let context = !self.driving_settings.google_mode
                        && notable(&place)
                        && now.duration_since(drive.last_context_callout)
                            >= Duration::from_secs(30);
                    if context {
                        drive.last_context_callout = now;
                    }
                    if !self.muted {
                        self.audio.cue(place.point, point, heading);
                    }
                    self.announce(&format!(
                        "Near {}.",
                        self.area.callout_place(i, point, heading)
                    ));
                    if context {
                        let _ = self.tx.send(Job::PlaceContext(place, true));
                    }
                }
            }
        }
        if arrived {
            let road_gap = self
                .drive
                .as_ref()
                .map(|drive| {
                    drive
                        .route
                        .points
                        .last()
                        .unwrap()
                        .distance(drive.route.destination)
                })
                .unwrap_or(0.);
            self.drive = None;
            self.persist();
            if road_gap > 50. {
                self.announce(&format!(
                    "Virtual drive complete. The road ends {:.0} metres from the selected place.",
                    road_gap
                ));
            } else {
                self.announce("Virtual drive complete.");
            }
        }
    }
    fn start(&mut self, job: Job) {
        if self.busy {
            self.announce(
                "A request is already running. You can keep exploring while it finishes.",
            );
            return;
        }
        self.busy = true;
        if matches!(job, Job::Load(_)) {
            self.map_generation += 1;
            self.route_generation += 1;
        }
        let sender = if matches!(job, Job::Load(_) | Job::Route(..) | Job::DriveRoute(..)) {
            &self.map_tx
        } else {
            &self.tx
        };
        if sender.send(job).is_err() {
            self.busy = false;
            self.announce("The download worker has stopped. Restart Streetwalk.");
            return;
        }
        self.announce("Loading. You can keep exploring while the request finishes.");
    }
    fn search(&mut self) {
        unsafe {
            let len = GetWindowTextLengthW(self.search);
            let mut value = vec![0u16; len as usize + 1];
            GetWindowTextW(self.search, value.as_mut_ptr(), len + 1);
            let query = String::from_utf16_lossy(&value[..len as usize]);
            if query.trim().is_empty() {
                self.announce("Enter a town, address, or latitude, longitude.");
                return;
            }
            let local_address = query.split(',').next().unwrap_or("").trim();
            if local_address
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit())
            {
                let matches: Vec<_> = self
                    .area
                    .places
                    .iter()
                    .filter(|place| {
                        place.category() == "Addresses"
                            && place.name.eq_ignore_ascii_case(local_address)
                    })
                    .map(|place| SearchResult {
                        name: format!("{} (mapped in current neighborhood)", place.name),
                        point: place.point,
                    })
                    .collect();
                if !matches.is_empty() {
                    self.fill_results(matches);
                    return;
                }
            }
            self.start(Job::Search(query, self.point));
        }
    }
    fn load_selected(&mut self) {
        if self.browse_mode == BrowseMode::Settings {
            self.adjust_settings(1);
            return;
        }
        if self.browse_mode == BrowseMode::Categories {
            self.select_category();
            return;
        }
        if self.browse_mode == BrowseMode::Preview {
            if let Some(road) = self
                .selected_choice_index()
                .and_then(|i| self.preview_options.get(i))
                .cloned()
            {
                self.move_to(road.destination, road.bearing);
                self.show_preview();
            } else {
                self.announce("Choose a road first.");
            }
            return;
        }
        if let Some(result) = self
            .selected_choice_index()
            .and_then(|i| self.choices.get(i))
            .cloned()
        {
            if matches!(
                self.browse_mode,
                BrowseMode::Places | BrowseMode::AllPlaces | BrowseMode::Explore
            ) {
                self.move_to(result.point, self.heading);
                self.show_view(View::Explore);
                unsafe { SetFocus(self.walk) };
                self.describe();
            } else {
                self.start(Job::Load(result));
            }
        } else {
            self.announce("Choose a result first.");
        }
    }
    fn fill_results(&mut self, choices: Vec<SearchResult>) {
        self.show_view(View::Search);
        self.browse_mode = BrowseMode::Search;
        self.choices = choices;
        self.page = 0;
        self.render_results_page();
        if !self.choices.is_empty() {
            unsafe { SetFocus(self.results) };
        }
        self.announce(&format!(
            "{} results in {} pages. {}",
            self.choices.len(),
            page_bounds(self.choices.len(), 0).3,
            if self.choices.is_empty() {
                "No matching numbered address or place found. Try a different spelling or nearby address."
            } else if self.destination_mode {
                "Enter plans a walking route from your current position. Page Up and Page Down browse more results."
            } else {
                "Enter moves here; Ctrl+Enter plans a walking route here. Use Page Up and Page Down for more results."
            }
        ));
    }
    fn show_categories(&mut self) {
        self.show_view(View::Nearby);
        self.categories = [
            "All places",
            "Crossings",
            "Addresses",
            "Food and drink",
            "Shopping",
            "Community and services",
            "Healthcare",
            "Transit",
            "Outdoors",
            "Sights",
            "Workplaces",
            "Other places",
        ]
        .iter()
        .filter(|category| {
            *category == &"All places"
                || self.area.places.iter().any(|p| p.category() == **category)
        })
        .map(|category| (*category).to_string())
        .collect();
        self.browse_mode = BrowseMode::Categories;
        self.choices.clear();
        self.page = 0;
        self.render_results_page();
        unsafe { SetFocus(self.results) };
        self.announce(
            "Categories. Use Up and Down across pages, then Enter to open a list of places.",
        );
    }
    fn select_category(&mut self) {
        let Some(category) = self
            .selected_choice_index()
            .and_then(|i| self.categories.get(i))
        else {
            return;
        };
        let category = category.clone();
        self.show_view(View::Nearby);
        let choices = self
            .area
            .nearby(self.point)
            .into_iter()
            .filter(|i| category == "All places" || self.area.places[*i].category() == category)
            .map(|i| {
                let place = &self.area.places[i];
                SearchResult {
                    name: self.area.describe_place(i, self.point, self.heading),
                    point: place.point,
                }
            })
            .collect::<Vec<_>>();
        self.browse_mode = BrowseMode::Places;
        self.choices = choices;
        self.page = 0;
        self.render_results_page();
        unsafe { SetFocus(self.results) };
        self.announce(&format!(
            "{}: {} places, nearest first. Up and Down browse across pages; Enter jumps; Ctrl+Enter routes.",
            category,
            self.choices.len()
        ));
        if !self.muted && unsafe { GetForegroundWindow() == self.hwnd } {
            if let Some(first) = self.choices.first() {
                self.cue_point(first.point);
            }
        }
    }
    fn poll(&mut self) {
        self.drive_tick();
        if self.drive.is_none() {
            self.audio.ev_stop();
        }
        self.audio.update(self.point, self.heading, unsafe {
            GetForegroundWindow() == self.hwnd
        });
        while let Ok(reply) = self.rx.try_recv() {
            if matches!(
                reply,
                Reply::Search(..)
                    | Reply::Load(..)
                    | Reply::Route(..)
                    | Reply::DriveRoute(..)
                    | Reply::PlaceContext(_, _, false)
            ) {
                self.busy = false;
            }
            match reply {
                Reply::Search(Ok(choices)) => self.fill_results(choices),
                Reply::Load(Ok((area, point))) => {
                    self.show_view(View::Explore);
                    self.audio.stop();
                    self.point = point;
                    self.area = area.normalize_legacy();
                    self.heading = 0.;
                    self.history.clear();
                    self.route = None;
                    self.drive = None;
                    self.last_weather = None;
                    self.last_weather_request = None;
                    self.spare.clear();
                    self.map_error = false;
                    self.last_cover = None;
                    self.announcer.reset(&self.area, self.point);
                    self.persist();
                    unsafe {
                        SetFocus(self.walk);
                    }
                    self.describe();
                }
                Reply::Search(Err(e)) | Reply::Load(Err(e)) => self.announce(&e),
                Reply::Cover(result, generation) => {
                    self.cover_pending = false;
                    if generation != self.map_generation {
                        continue;
                    }
                    match result {
                        Ok(area) => {
                            self.map_error = false;
                            if area.contains(self.point)
                                && (!self.area.contains(self.point)
                                    || area.version > self.area.version
                                    || area.center.distance(self.point)
                                        < self.area.center.distance(self.point))
                            {
                                self.swap_area(area);
                                if self.drive.is_none() { self.passive_update(); }
                            } else {
                                self.spare.push(area);
                                if self.spare.len() > 8 {
                                    self.spare.remove(0);
                                }
                            }
                        }
                        Err(_) => {
                            if !self.map_error {
                                self.announce("Map download unavailable. Cached maps and route following still work; automatic retry in a minute.");
                            }
                            self.map_error = true;
                            self.last_cover = Some(Instant::now());
                        }
                    }
                }
                Reply::Route(result, start, generation) => {
                    if generation != self.route_generation {
                        continue;
                    }
                    if self.point.distance(start) > 25. {
                        self.announce("You moved while the route was calculated. Request the route again from your new position.");
                        continue;
                    }
                    match result {
                        Ok(r) => self.activate_route(r),
                        Err(e) => self.announce(&e),
                    }
                }
                Reply::DriveRoute(result, start, generation) => {
                    if generation != self.drive_generation { continue; }
                    if self.point.distance(start) > 25. { self.announce("Your position changed while the drive route was calculated. Request it again."); continue; }
                    match result {
                        Ok(route) => {
                            let start_limit = if self.driving_settings.google_mode { 500. } else { 100. };
                            if route.points[0].distance(self.point) > start_limit { self.announce("No drivable road begins near this position."); continue; }
                            let name = route.name.clone();
                            let length = route.length();
                            let (start_point, start_heading) = route.position(0.);
                            let (speed_kmh, tagged_speed) = if self.driving_settings.google_mode {
                                (route.cruise_speed(0.).unwrap_or(self.driving_settings.fallback.suburban), false)
                            } else {
                                self.area.driving_speed(start_point, start_heading, self.driving_settings.fallback)
                            };
                            let start_context = self.area.context(start_point).map(|(key, _)| key);
                            let departure = route.maneuvers.first().map(|m| m.text.clone()).unwrap_or_default();
                            self.route = None;
                            self.drive = Some(Drive::new(route, speed_kmh, tagged_speed, self.area.driving_texture(start_point), start_context));
                            self.traffic_generation = self.traffic_generation.wrapping_add(1);
                            self.show_view(View::Explore);
                            unsafe { SetFocus(self.walk) };
                            self.announce(&format!("Virtual drive to {name}, {:.1} kilometres. {departure}. Accelerating toward {:.0} kilometres per hour. D pauses or resumes; U ends the drive.", length / 1000., speed_kmh));
                            self.drive_address_pending = false;
                            self.last_drive_address_request = None;
                            self.pending_drive_address = None;
                            self.request_drive_address(true);
                            self.request_weather(true);
                            self.last_traffic_request = None;
                            self.last_traffic = None;
                            self.traffic_pending = false;
                            self.traffic_error_reported = false;
                            self.request_traffic(true);
                            self.incidents_pending = false;
                            self.last_incidents_request = None;
                            self.incidents.clear();
                            self.incidents_error_reported = false;
                            self.request_incidents(true);
                        }
                        Err(e) => self.announce(&e),
                    }
                }
                Reply::PlaceContext(name, result, during_drive) => match result {
                    Ok((description, source)) => {
                        if !during_drive || self.drive.is_some() {
                            self.announce(&if during_drive { description } else { format!("{name}: {description}") });
                            if source.starts_with("https://en.wikipedia.org/") {
                                self.append_transcript(&format!("Source: {source}. Wikipedia text, CC BY-SA 4.0."));
                            } else if !source.is_empty() {
                                self.append_transcript(&format!("Source: {source}"));
                            }
                        }
                    }
                    Err(e) if !during_drive => self.announce(&e),
                    Err(_) => {},
                },
                Reply::Weather(result, point, spoken) => {
                    self.weather_pending = false;
                    if self.point.distance(point) > 5_000. { continue; }
                    match result {
                        Ok(weather) => {
                            let changed = self.last_weather.as_ref().is_none_or(|old| weather.changed_from(old));
                            if spoken || (self.drive.is_some() && changed) {
                                self.announce(&format!("{}: {}.", if spoken { "Current modeled weather" } else { "Weather along the route" }, weather.summary()));
                                self.append_transcript("Source: Open-Meteo, https://open-meteo.com/. Weather data CC BY 4.0.");
                            }
                            self.last_weather = Some(weather);
                        }
                        Err(error) if spoken => self.announce(&error),
                        Err(_) => {},
                    }
                }
                Reply::Elevations(result, positions, generation) => {
                    if generation != self.drive_generation { continue; }
                    if let Some(drive) = &mut self.drive {
                        drive.elevation_pending = false;
                        match result {
                            Ok(heights) => {
                                drive.elevation_samples.retain(|(at, _)| *at < positions[0]);
                                drive.elevation_samples.extend(positions.iter().copied().zip(heights));
                                drive.elevation_until = positions.last().copied().unwrap_or(drive.progress);
                            }
                            Err(_) => drive.elevation_until = drive.progress + 5_000.,
                        }
                    }
                }
                Reply::Traffic(result, sample_point, heading, generation) => {
                    if generation != self.traffic_generation { continue; }
                    self.traffic_pending = false;
                    if !self.driving_settings.live_traffic { continue; }
                    if self.drive.is_none() || self.point.distance(sample_point) > 2_000. { continue; }
                    match result {
                        Ok(flow) if flow.matches_route(sample_point, heading) => {
                            let congested = flow.confidence >= 0.4 && flow.current_kmh < flow.free_kmh * 0.8;
                            let previous = self.last_traffic.as_ref().is_some_and(|old| old.confidence >= 0.4 && old.current_kmh < old.free_kmh * 0.8);
                            if let Some(drive) = &mut self.drive {
                                if flow.closed {
                                    drive.paused = true;
                                    drive.paused_by_closure = true;
                                    drive.traffic_kmh = None;
                                    self.announce("TomTom reports a road closure on this segment. Virtual drive paused. Choose another route or turn off live traffic in Driving options.");
                                } else {
                                    drive.traffic_kmh = if congested { Some(flow.current_kmh.max(5.)) } else { None };
                                    if congested && drive.lead.is_none() {
                                        drive.lead = Some(LeadTraffic::new(drive.progress, drive.actual_speed_kmh, drive.driver.headway_seconds));
                                    } else if !congested {
                                        drive.lead = None;
                                    }
                                    if congested && (!previous || self.last_traffic.as_ref().is_some_and(|old| old.current_kmh - flow.current_kmh > 15.)) {
                                        self.announce(&format!("Live traffic is slow here: about {:.0} kilometres per hour, versus {:.0} in free flow.", flow.current_kmh, flow.free_kmh));
                                    } else if previous && !congested {
                                        self.announce("Live traffic has cleared. Returning to the mapped or estimated road speed.");
                                    }
                                }
                            }
                            self.last_traffic = Some(flow);
                            self.append_transcript("Traffic source: TomTom Traffic Flow API. Speeds are current estimates for a nearby road segment.");
                        }
                        Ok(_) => {
                            if let Some(drive) = &mut self.drive { drive.traffic_kmh = None; drive.lead = None; }
                            self.last_traffic = None;
                        }
                        Err(e) if !self.traffic_error_reported => {
                            self.traffic_error_reported = true;
                            self.announce(&format!("Live traffic unavailable: {e}. Continuing with mapped or estimated speed."));
                        }
                        Err(_) => {}
                    }
                }
                Reply::Incidents(result, generation) => {
                    if generation != self.traffic_generation { continue; }
                    self.incidents_pending = false;
                    if !self.driving_settings.live_traffic { continue; }
                    match result {
                        Ok(incidents) => self.incidents = incidents,
                        Err(error) if !self.incidents_error_reported => {
                            self.incidents_error_reported = true;
                            self.announce(&format!("Traffic incidents unavailable: {error}. The virtual drive continues."));
                        }
                        Err(_) => {}
                    }
                }
                Reply::DriveAddress(result, sample, generation) => {
                    if generation != self.drive_generation { continue; }
                    self.drive_address_pending = false;
                    if self.drive.is_none() || self.point.distance(sample) > 600. { continue; }
                    let address = result.unwrap_or_default();
                    if self.driving_settings.google_mode && !address.street.is_empty() {
                        self.area.google_street = address.street.clone();
                    }
                    self.pending_drive_address = drive_address_phrase(&self.area, sample, &address);
                }
                Reply::GoogleWalkAddress(result, sample, generation) => {
                    if generation != self.map_generation { continue; }
                    self.google_walk_pending = false;
                    if !self.driving_settings.google_mode || self.drive.is_some() || self.point.distance(sample) > 200. { continue; }
                    if let Ok(address) = result {
                        if !address.street.is_empty() && address.street != self.area.google_street {
                            self.area.google_street = address.street.clone();
                            self.announce(&format!("On {}.", address.street));
                        }
                    }
                }
                Reply::PcLocation(..) if !self.start_at_pc_location => {}
                Reply::PcLocation(result, origin) => match result {
                    Ok(_) if self.point.distance(origin) > 2. => self.announce(
                        "PC location is ready, but you moved in the virtual map. Press Ctrl+L to request it again.",
                    ),
                    Ok(point) => {
                        if self.busy {
                            self.announce("PC location is ready. Finish the current download, then press Ctrl+L to jump there.");
                        } else {
                            self.announce("PC location found. Loading its neighborhood.");
                            self.start(Job::Load(SearchResult {
                                name: "PC location".into(),
                                point,
                            }));
                        }
                    }
                    Err(e) => self.announce(&e),
                },
                Reply::WhereAmI(result, point, heading) => {
                    if self.point.distance(point) <= 5. {
                        let address = result.unwrap_or_default();
                        self.announce(&self.area.full_location(point, heading, &address));
                    }
                }
            }
        }
        self.ensure_coverage(false);
    }
    fn key(&mut self, key: u16, shift: bool) {
        match key {
            VK_UP | VK_RIGHT | VK_DOWN | VK_LEFT => {
                self.walking_step();
                let heading = match key {
                    VK_UP => 0.,
                    VK_RIGHT => 90.,
                    VK_DOWN => 180.,
                    _ => 270.,
                };
                self.move_to(
                    self.point
                        .walk(heading, [1., 5., 10., 25., 50., 100.][self.step]),
                    heading,
                );
            }
            VK_OEM_PLUS | VK_ADD | VK_OEM_6 => {
                self.step = (self.step + 1).min(5);
                self.announce(&format!(
                    "Step {} metres",
                    [1, 5, 10, 25, 50, 100][self.step]
                ));
            }
            VK_OEM_MINUS | VK_SUBTRACT | VK_OEM_4 => {
                self.step = self.step.saturating_sub(1);
                self.announce(&format!(
                    "Step {} metres",
                    [1, 5, 10, 25, 50, 100][self.step]
                ));
            }
            VK_SPACE => {
                if shift {
                    self.where_am_i();
                } else {
                    self.describe();
                }
            }
            0x51 | 0x45 => {
                let turn = if shift { 1. } else { self.turn };
                self.heading =
                    (self.heading + if key == 0x51 { -turn } else { turn }).rem_euclid(360.);
                self.audio.update(self.point, self.heading, unsafe {
                    GetForegroundWindow() == self.hwnd
                });
                unsafe {
                    text(self.heading_input, &format!("{:.0}", self.heading));
                }
                self.announce(&format!(
                    "{} {:.0} degrees. Facing {} at {:.0} degrees",
                    if key == 0x51 { "Left" } else { "Right" },
                    turn,
                    crate::geo::compass(self.heading),
                    self.heading
                ));
            }
            0x57 | 0x58 => {
                self.walking_step();
                let bearing = if key == 0x58 {
                    self.heading + 180.
                } else {
                    self.heading
                };
                self.move_to(
                    self.point
                        .walk(bearing, [1., 5., 10., 25., 50., 100.][self.step]),
                    self.heading,
                );
            }
            0x47 => {
                if shift {
                    if let Some(g) = &self.route {
                        self.plan(SearchResult {
                            name: g.route.name.clone(),
                            point: g.route.destination,
                        });
                    } else {
                        self.announce("No route to recalculate.");
                    }
                } else {
                    self.show_all_places();
                    self.announce("Select a POI, then press G or Ctrl+Enter to route there.");
                }
            }
            0x46 => {
                if let Some(g) = &self.route {
                    let (p, h) = g.advance(self.point, [1., 5., 10., 25., 50., 100.][self.step]);
                    self.walking_step();
                    self.move_to(p, h);
                } else {
                    self.announce("No active route. Ctrl+D finds a destination.");
                }
            }
            0x54 => {
                if let Some(g) = &self.route {
                    self.announce(&if shift { g.route.summary() } else { g.next() });
                } else {
                    self.announce("No active route.");
                }
            }
            0x55 => {
                if self.drive.take().is_some() {
                    self.persist();
                    self.announce("Virtual drive ended. You can explore from here.");
                    return;
                }
                if self
                    .route
                    .as_ref()
                    .is_some_and(|g| self.audio.target() == Some(g.route.destination))
                {
                    self.audio.stop();
                }
                self.route = None;
                self.route_generation += 1;
                self.show_view(View::Explore);
                unsafe { SetFocus(self.walk) };
                self.announce("Route cancelled.");
            }
            0x50 => self.show_all_places(),
            0x49 => self.place_info(),
            0x59 => self.request_weather(true),
            0x5A => self.show_settings(),
            0x44 => {
                if let Some(drive) = &mut self.drive {
                    drive.paused = !drive.paused;
                    drive.paused_by_closure = false;
                    drive.last_tick = Instant::now();
                    let paused = drive.paused;
                    self.announce(if paused {
                        "Virtual drive paused."
                    } else {
                        "Virtual drive resumed."
                    });
                } else {
                    self.announce(
                        "Select a destination in a place or search list, then choose Drive tour.",
                    );
                }
            }
            0x4B => {
                if self.muted {
                    self.announce("Sound is muted. Press M to turn speech and spatial cues on.");
                    return;
                }
                let listed = if matches!(self.view, View::Nearby | View::Preview)
                    && matches!(
                        self.browse_mode,
                        BrowseMode::Places
                            | BrowseMode::AllPlaces
                            | BrowseMode::Explore
                            | BrowseMode::Preview
                    ) {
                    self.selected_choice_index()
                        .and_then(|i| self.choices.get(i))
                        .map(|c| (c.point, c.name.clone()))
                } else {
                    None
                };
                let target = listed.or_else(|| {
                    self.route
                        .as_ref()
                        .map(|g| (g.route.destination, g.route.name.clone()))
                });
                if let Some((point, name)) = target {
                    if !self.audio.available() {
                        self.announce("No audio output device available for a beacon.");
                    } else if self.audio.toggle_beacon(point, self.point, self.heading) {
                        self.announce(&format!(
                            "Audio beacon on: {}. Press K again to stop.",
                            name
                        ));
                    } else {
                        self.announce("Audio beacon off.");
                    }
                } else if self.audio.target().is_some() {
                    self.audio.stop();
                    self.announce("Audio beacon off.");
                } else {
                    self.announce("Choose a nearby place or start a route, then press K for its audio beacon.");
                }
            }
            0x4E => self.show_categories(),
            0x56 => self.show_preview(),
            0x4F => self.explore(false),
            0x41 => self.explore(true),
            VK_RETURN => self.show_all_places(),
            0x53 => {
                if let Some((_, q, d)) = self.area.nearest_road(self.point) {
                    if d <= 100. {
                        self.move_to(q, self.heading);
                    } else {
                        self.announce("No mapped street within 100 metres.");
                    }
                } else {
                    self.announce("No mapped streets in this neighborhood.");
                }
            }
            0x48 => self.move_to(self.area.center, self.heading),
            VK_BACK => {
                if let Some((p, h)) = self.history.pop() {
                    self.point = p;
                    self.heading = h;
                    if let Some(g) = &mut self.route {
                        let route = g.route.clone();
                        *g = Guidance::new(route);
                    }
                    self.passive_update();
                    self.ensure_coverage(false);
                } else {
                    self.announce("No earlier position.");
                }
            }
            0x43 => self.announce(&format!(
                "Latitude {:.6}, longitude {:.6}",
                self.point.lat, self.point.lon
            )),
            0x4C => {
                let lines: Vec<_> = all_poi_choices(&self.area, self.point, self.heading)
                    .into_iter()
                    .take(10)
                    .map(|choice| choice.name)
                    .collect();
                self.announce(&spoken_places(&lines));
            }
            0x52 => {
                self.ensure_coverage(true);
            }
            0x42 => {
                let mut bookmarks: Vec<SearchResult> =
                    data::read("bookmarks.json").unwrap_or_default();
                bookmarks.push(SearchResult {
                    name: format!(
                        "{}: {:.5}, {:.5}",
                        self.area.name, self.point.lat, self.point.lon
                    ),
                    point: self.point,
                });
                let saved = data::save(
                    &format!("area_v9_{:.5}_{:.5}.json", self.point.lat, self.point.lon),
                    &self.area,
                )
                .and_then(|_| data::save("bookmarks.json", &bookmarks));
                match saved{Ok(())=>self.announce("Bookmark and its neighborhood saved for offline use. Press J to browse bookmarks."),Err(e)=>self.announce(&e)}
            }
            0x4A => {
                self.show_saved();
            }
            0x4D => {
                self.muted = !self.muted;
                if self.muted {
                    self.speech.stop();
                    self.audio.stop();
                }
                self.announce(if self.muted {
                    "Automatic speech and spatial cues off."
                } else {
                    "Automatic speech and spatial cues on."
                });
            }
            VK_F1 => self.help(),
            _ => {}
        }
    }
}
unsafe extern "system" fn proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if msg == WM_COMMAND {
        PostMessageW(hwnd, WM_APP + 1, w, l);
        return 0;
    }
    if msg == WM_DESTROY {
        PostQuitMessage(0);
        return 0;
    }
    if msg == WM_CLOSE {
        DestroyWindow(hwnd);
        return 0;
    }
    DefWindowProcW(hwnd, msg, w, l)
}
pub fn run() {
    unsafe {
        let _ = windows::Win32::System::WinRT::RoInitialize(
            windows::Win32::System::WinRT::RO_INIT_SINGLETHREADED,
        );
        let instance = GetModuleHandleW(null_mut());
        let class = wide("StreetwalkWindow");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hbrBackground: (COLOR_WINDOW + 1) as HBRUSH,
            ..std::mem::zeroed()
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            WS_EX_CONTROLPARENT,
            class.as_ptr(),
            wide("Streetwalk — virtual exploration").as_ptr(),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            820,
            740,
            null_mut(),
            null_mut(),
            instance,
            null_mut(),
        );
        if hwnd.is_null() {
            MessageBoxW(
                null_mut(),
                wide("Could not create Streetwalk window.").as_ptr(),
                wide("Streetwalk").as_ptr(),
                MB_OK,
            );
            return;
        }
        let view_title = control(hwnd, "STATIC", "Explore", 0, 0, 20, 12, 760, 24);
        control(
            hwnd,
            "BUTTON",
            "My location (Space)",
            WS_TABSTOP,
            MY_LOCATION,
            20,
            40,
            125,
            38,
        );
        control(
            hwnd,
            "BUTTON",
            "Around me (O)",
            WS_TABSTOP,
            AROUND_ME,
            149,
            40,
            108,
            38,
        );
        control(
            hwnd,
            "BUTTON",
            "Ahead of me (A)",
            WS_TABSTOP,
            AHEAD_OF_ME,
            261,
            40,
            124,
            38,
        );
        control(
            hwnd,
            "BUTTON",
            "Nearby (N)",
            WS_TABSTOP,
            NEARBY,
            389,
            40,
            94,
            38,
        );
        control(
            hwnd,
            "BUTTON",
            "Search (Ctrl+F)",
            WS_TABSTOP,
            SEARCH_ACTION,
            487,
            40,
            114,
            38,
        );
        control(
            hwnd,
            "BUTTON",
            "Route",
            WS_TABSTOP,
            ROUTE_ACTION,
            605,
            40,
            74,
            38,
        );
        control(
            hwnd,
            "BUTTON",
            "Saved (J)",
            WS_TABSTOP,
            SAVED_ACTION,
            683,
            40,
            92,
            38,
        );
        let search_label = control(
            hwnd,
            "STATIC",
            "Search a town, place, address, or latitude, longitude",
            0,
            0,
            20,
            82,
            760,
            24,
        );
        let search = control(
            hwnd,
            "EDIT",
            "",
            WS_TABSTOP | WS_BORDER | ES_AUTOHSCROLL as u32,
            SEARCH,
            20,
            110,
            610,
            28,
        );
        let find_button = control(
            hwnd,
            "BUTTON",
            "Find (Enter)",
            WS_TABSTOP,
            FIND,
            645,
            110,
            130,
            28,
        );
        let results_label = control(
            hwnd,
            "STATIC",
            "Places and search results — Up/Down to browse; Enter to jump; Ctrl+Enter to route",
            0,
            0,
            20,
            144,
            510,
            22,
        );
        let page_previous = control(
            hwnd,
            "BUTTON",
            "Previous page",
            WS_TABSTOP,
            PAGE_PREVIOUS,
            540,
            144,
            110,
            24,
        );
        let page_next = control(
            hwnd,
            "BUTTON",
            "Next page",
            WS_TABSTOP,
            PAGE_NEXT,
            660,
            144,
            115,
            24,
        );
        let results = control(
            hwnd,
            "LISTBOX",
            "",
            WS_TABSTOP | WS_BORDER | WS_VSCROLL | LBS_NOTIFY as u32,
            RESULTS,
            20,
            168,
            400,
            96,
        );
        let load_button = control(
            hwnd,
            "BUTTON",
            "Jump (Enter)",
            WS_TABSTOP,
            LOAD,
            645,
            168,
            130,
            32,
        );
        let route_button = control(
            hwnd,
            "BUTTON",
            "Route (Ctrl+Enter)",
            WS_TABSTOP,
            ROUTE,
            645,
            202,
            130,
            32,
        );
        let fix_button = control(
            hwnd,
            "BUTTON",
            "Fix the map",
            WS_TABSTOP,
            FIX_MAP,
            645,
            236,
            130,
            28,
        );
        let place_info_button = control(
            hwnd,
            "BUTTON",
            "More about place (I)",
            WS_TABSTOP,
            PLACE_INFO,
            430,
            168,
            200,
            32,
        );
        let drive_button = control(
            hwnd,
            "BUTTON",
            "Drive tour (Ctrl+Shift+Enter)",
            WS_TABSTOP,
            DRIVE_TOUR,
            430,
            202,
            200,
            32,
        );
        let route_title = control(hwnd, "STATIC", "No active route", 0, 0, 20, 82, 510, 24);
        let route_list = control(
            hwnd,
            "LISTBOX",
            "",
            WS_TABSTOP | WS_BORDER | WS_VSCROLL,
            0,
            20,
            110,
            610,
            154,
        );
        let route_follow = control(
            hwnd,
            "BUTTON",
            "Follow (F)",
            WS_TABSTOP,
            ROUTE_FOLLOW,
            645,
            110,
            130,
            32,
        );
        let route_recalc = control(
            hwnd,
            "BUTTON",
            "Recalc (Shift+G)",
            WS_TABSTOP,
            ROUTE_RECALC,
            645,
            149,
            130,
            32,
        );
        let route_cancel = control(
            hwnd,
            "BUTTON",
            "Cancel (U)",
            WS_TABSTOP,
            ROUTE_CANCEL,
            645,
            188,
            130,
            32,
        );
        let route_beacon = control(
            hwnd,
            "BUTTON",
            "Beacon (K)",
            WS_TABSTOP,
            ROUTE_BEACON,
            645,
            227,
            130,
            32,
        );
        let walk = control(
            hwnd,
            "BUTTON",
            "Virtual walk — Escape to focus; arrows move; Space describes; F1 for help",
            WS_TABSTOP,
            WALK,
            20,
            276,
            755,
            40,
        );
        let heading_label = control(hwnd, "STATIC", "Heading degrees", 0, 0, 20, 328, 112, 24);
        let heading_input = control(
            hwnd,
            "EDIT",
            "0",
            WS_TABSTOP | WS_BORDER | ES_AUTOHSCROLL as u32,
            HEADING,
            136,
            324,
            66,
            28,
        );
        let face_button = control(hwnd, "BUTTON", "Face", WS_TABSTOP, FACE, 210, 324, 75, 28);
        let turn_label = control(hwnd, "STATIC", "Turn degrees", 0, 0, 310, 328, 105, 24);
        let turn_input = control(
            hwnd,
            "EDIT",
            "15",
            WS_TABSTOP | WS_BORDER | ES_AUTOHSCROLL as u32,
            TURN,
            420,
            324,
            66,
            28,
        );
        let apply_button = control(hwnd, "BUTTON", "Apply", WS_TABSTOP, APPLY, 496, 324, 75, 28);
        let walk_beacon = control(
            hwnd,
            "BUTTON",
            "Beacon (K)",
            WS_TABSTOP,
            BEACON_ACTION,
            600,
            324,
            82,
            28,
        );
        let road_preview = control(
            hwnd,
            "BUTTON",
            "Roads (V)",
            WS_TABSTOP,
            STREET_PREVIEW,
            690,
            324,
            85,
            28,
        );
        let walk_hint = control(
            hwnd,
            "STATIC",
            "P: POIs | Q/E: turn | W/X: walk",
            0,
            0,
            20,
            369,
            400,
            25,
        );
        let drive_options_button = control(
            hwnd,
            "BUTTON",
            "Map and driving options (Z)",
            WS_TABSTOP,
            DRIVE_OPTIONS,
            430,
            366,
            210,
            28,
        );
        let options_next_button = control(
            hwnd,
            "BUTTON",
            "Increase (Enter or Right)",
            WS_TABSTOP,
            OPTIONS_NEXT,
            645,
            110,
            130,
            32,
        );
        let options_prev_button = control(
            hwnd,
            "BUTTON",
            "Decrease (Left)",
            WS_TABSTOP,
            OPTIONS_PREV,
            645,
            149,
            130,
            32,
        );
        let weather_button = control(
            hwnd,
            "BUTTON",
            "Weather (Y)",
            WS_TABSTOP,
            WEATHER_ACTION,
            650,
            366,
            125,
            28,
        );
        let transcript_label = control(
            hwnd,
            "STATIC",
            "Announcement transcript",
            0,
            0,
            20,
            405,
            760,
            22,
        );
        let output = control(
            hwnd,
            "EDIT",
            "",
            WS_TABSTOP
                | WS_BORDER
                | WS_VSCROLL
                | ES_MULTILINE as u32
                | ES_READONLY as u32
                | ES_AUTOVSCROLL as u32,
            STATUS,
            20,
            430,
            755,
            188,
        );
        let map_attribution = control(
            hwnd,
            "STATIC",
            "Map data © OpenStreetMap contributors · ODbL · openstreetmap.org/copyright",
            0,
            0,
            20,
            630,
            760,
            24,
        );
        let route_attribution = control(
            hwnd,
            "STATIC",
            "Routes: FOSSGIS / OSRM | Weather: Open-Meteo | Fix map: openstreetmap.org/fixthemap",
            0,
            0,
            20,
            655,
            760,
            24,
        );
        let (tx, jobs) = mpsc::channel();
        let (map_tx, map_jobs) = mpsc::channel();
        let (send, rx) = mpsc::channel();
        let location_tx = send.clone();
        let map_send = send.clone();
        std::thread::spawn(move || {
            let mut network = Network::new();
            while let Ok(job) = map_jobs.recv() {
                let reply = match job {
                    Job::SetGoogleMode(enabled) => {
                        if let Ok(network) = &mut network {
                            network.set_google_mode(enabled);
                        }
                        continue;
                    }
                    Job::Load(place) => {
                        let point = place.point;
                        Reply::Load(match &mut network {
                            Ok(network) => network.area(place).map(|area| (area, point)),
                            Err(error) => Err(error.clone()),
                        })
                    }
                    Job::Cover(point, generation) => Reply::Cover(
                        match &mut network {
                            Ok(network) => network.cover(point),
                            Err(error) => Err(error.clone()),
                        },
                        generation,
                    ),
                    Job::Route(start, destination, generation) => Reply::Route(
                        match &mut network {
                            Ok(network) => network.route(start, destination),
                            Err(error) => Err(error.clone()),
                        },
                        start,
                        generation,
                    ),
                    Job::DriveRoute(start, destination, preference, generation) => {
                        Reply::DriveRoute(
                            match &mut network {
                                Ok(network) => network.route_drive(start, destination, preference),
                                Err(error) => Err(error.clone()),
                            },
                            start,
                            generation,
                        )
                    }
                    _ => continue,
                };
                if map_send.send(reply).is_err() {
                    break;
                }
            }
        });
        std::thread::spawn(move || {
            let mut network = Network::new();
            while let Ok(job) = jobs.recv() {
                let reply = match job {
                    Job::SetGoogleMode(enabled) => {
                        if let Ok(network) = &mut network {
                            network.set_google_mode(enabled);
                        }
                        continue;
                    }
                    Job::Search(q, origin) => Reply::Search(match &mut network {
                        Ok(n) => n.search_at(&q, Some(origin)),
                        Err(e) => Err(e.clone()),
                    }),
                    Job::Load(p) => {
                        let point = p.point;
                        Reply::Load(match &mut network {
                            Ok(n) => n.area(p).map(|a| (a, point)),
                            Err(e) => Err(e.clone()),
                        })
                    }
                    Job::Cover(p, generation) => Reply::Cover(
                        match &mut network {
                            Ok(n) => n.cover(p),
                            Err(e) => Err(e.clone()),
                        },
                        generation,
                    ),
                    Job::Route(start, destination, generation) => Reply::Route(
                        match &mut network {
                            Ok(n) => n.route(start, destination),
                            Err(e) => Err(e.clone()),
                        },
                        start,
                        generation,
                    ),
                    Job::DriveRoute(start, destination, preference, generation) => {
                        Reply::DriveRoute(
                            match &mut network {
                                Ok(n) => n.route_drive(start, destination, preference),
                                Err(e) => Err(e.clone()),
                            },
                            start,
                            generation,
                        )
                    }
                    Job::PlaceContext(place, during_drive) => Reply::PlaceContext(
                        place.name.clone(),
                        match &mut network {
                            Ok(n) => n.place_context(&place),
                            Err(e) => Err(e.clone()),
                        },
                        during_drive,
                    ),
                    Job::Weather(point, spoken) => Reply::Weather(
                        match &mut network {
                            Ok(n) => n.weather(point),
                            Err(e) => Err(e.clone()),
                        },
                        point,
                        spoken,
                    ),
                    Job::Elevations(samples, generation) => {
                        let positions = samples.iter().map(|(at, _)| *at).collect();
                        Reply::Elevations(
                            match &mut network {
                                Ok(n) => n.elevations(
                                    &samples.iter().map(|(_, point)| *point).collect::<Vec<_>>(),
                                ),
                                Err(e) => Err(e.clone()),
                            },
                            positions,
                            generation,
                        )
                    }
                    Job::Traffic(point, heading, generation) => Reply::Traffic(
                        match &mut network {
                            Ok(n) => n.traffic_flow(point),
                            Err(e) => Err(e.clone()),
                        },
                        point,
                        heading,
                        generation,
                    ),
                    Job::Incidents(point, generation) => Reply::Incidents(
                        match &mut network {
                            Ok(n) => n.traffic_incidents(point),
                            Err(e) => Err(e.clone()),
                        },
                        generation,
                    ),
                    Job::DriveAddress(point, generation) => Reply::DriveAddress(
                        match &mut network {
                            Ok(n) => n.reverse_address(point),
                            Err(e) => Err(e.clone()),
                        },
                        point,
                        generation,
                    ),
                    Job::GoogleWalkAddress(point, generation) => Reply::GoogleWalkAddress(
                        match &mut network {
                            Ok(n) => n.reverse_address(point),
                            Err(e) => Err(e.clone()),
                        },
                        point,
                        generation,
                    ),
                    Job::WhereAmI(point, heading) => Reply::WhereAmI(
                        match &mut network {
                            Ok(n) => n.reverse_address(point),
                            Err(e) => Err(e.clone()),
                        },
                        point,
                        heading,
                    ),
                };
                if send.send(reply).is_err() {
                    break;
                }
            }
        });
        let session = data::read::<Session>("session.json").ok().filter(|s| {
            s.point.valid()
                && s.step < 6
                && s.heading.is_finite()
                && s.turn.is_finite()
                && (1.0..=180.).contains(&s.turn)
        });
        let (area, point, heading, step, turn, route) = if let Some(s) = session {
            (
                s.area.normalize_legacy(),
                s.point,
                s.heading,
                s.step,
                s.turn,
                s.route.and_then(|r| r.validate().ok()).map(Guidance::new),
            )
        } else {
            let a = Area::demo();
            let p = a.center;
            (a, p, 0., 2, 15., None)
        };
        let driving_settings = data::read::<DrivingSettings>("driving_settings.json")
            .ok()
            .filter(DrivingSettings::valid)
            .unwrap_or_default();
        let google_mode = driving_settings.google_mode && crate::google::key().is_ok();
        let mut app = App {
            hwnd,
            view_title,
            search,
            results,
            walk,
            output,
            map_attribution,
            route_attribution,
            route_title,
            route_list,
            page_previous,
            page_next,
            heading_input,
            turn_input,
            area,
            point,
            heading,
            step,
            history: vec![],
            choices: vec![],
            page: 0,
            route_page: 0,
            preview_options: vec![],
            browse_mode: BrowseMode::Search,
            categories: vec![],
            speech: Speech::new(),
            audio: Audio::new(),
            muted: false,
            busy: false,
            tx,
            map_tx,
            rx,
            location_tx,
            start_at_pc_location: false,
            announcer: Announcer::default(),
            route,
            drive: None,
            drive_generation: 0,
            traffic_generation: 0,
            driving_settings,
            weather_pending: false,
            traffic_pending: false,
            last_traffic_request: None,
            last_traffic: None,
            traffic_error_reported: false,
            incidents_pending: false,
            last_incidents_request: None,
            incidents: vec![],
            incidents_error_reported: false,
            drive_address_pending: false,
            last_drive_address_request: None,
            pending_drive_address: None,
            google_walk_pending: false,
            last_google_walk_request: None,
            last_weather_request: None,
            last_weather: None,
            turn,
            destination_mode: false,
            spare: vec![],
            map_generation: 0,
            route_generation: 0,
            cover_pending: false,
            last_cover: None,
            map_error: false,
            last_walk: None,
            view: View::Search,
            search_controls: vec![
                Positioned::new(search_label, 20, 82, 760, 24),
                Positioned::new(search, 20, 110, 610, 28),
                Positioned::new(find_button, 645, 110, 130, 28),
            ],
            results_controls: vec![
                Positioned::new(results_label, 20, 144, 510, 22),
                Positioned::new(results, 20, 168, 400, 96),
                Positioned::new(load_button, 645, 168, 130, 32),
                Positioned::new(route_button, 645, 202, 130, 32),
                Positioned::new(fix_button, 645, 236, 130, 28),
                Positioned::new(place_info_button, 430, 168, 200, 32),
                Positioned::new(drive_button, 430, 202, 200, 32),
            ],
            walking_controls: vec![
                Positioned::new(walk, 20, 276, 755, 40),
                Positioned::new(heading_label, 20, 328, 112, 24),
                Positioned::new(heading_input, 136, 324, 66, 28),
                Positioned::new(face_button, 210, 324, 75, 28),
                Positioned::new(turn_label, 310, 328, 105, 24),
                Positioned::new(turn_input, 420, 324, 66, 28),
                Positioned::new(apply_button, 496, 324, 75, 28),
                Positioned::new(walk_beacon, 600, 324, 82, 28),
                Positioned::new(road_preview, 690, 324, 85, 28),
                Positioned::new(walk_hint, 20, 369, 400, 25),
                Positioned::new(drive_options_button, 430, 366, 210, 28),
                Positioned::new(weather_button, 650, 366, 125, 28),
                Positioned::new(transcript_label, 20, 405, 760, 22),
                Positioned::new(output, 20, 430, 755, 188),
            ],
            route_controls: vec![
                Positioned::new(route_title, 20, 82, 510, 24),
                Positioned::new(route_list, 20, 110, 610, 154),
                Positioned::new(route_follow, 645, 110, 130, 32),
                Positioned::new(route_recalc, 645, 149, 130, 32),
                Positioned::new(route_cancel, 645, 188, 130, 32),
                Positioned::new(route_beacon, 645, 227, 130, 32),
            ],
            settings_controls: vec![
                Positioned::new(options_next_button, 645, 110, 130, 32),
                Positioned::new(options_prev_button, 645, 149, 130, 32),
            ],
            announcements: RefCell::new(VecDeque::new()),
        };
        app.show_view(View::Explore);
        app.update_attribution();
        if app.driving_settings.google_mode && !google_mode {
            app.driving_settings.google_mode = false;
            app.announce("Google Maps key is missing; using OpenStreetMap. Add google_key.txt and select Google Maps in options.");
        }
        if google_mode {
            app.area = Area::google_empty(app.point);
            app.route = None;
            let _ = app.tx.send(Job::SetGoogleMode(true));
            let _ = app.map_tx.send(Job::SetGoogleMode(true));
            app.start(Job::Load(SearchResult {
                name: "Current position".into(),
                point: app.point,
            }));
        }
        app.announcer.reset(&app.area, app.point);
        let refresh_old_area = !app.area.is_demo() && app.area.version < 9;
        if let Some(g) = &mut app.route {
            g.progress = g.route.locate(app.point, 0.).0;
        }
        text(heading_input, &format!("{:.0}", heading));
        text(turn_input, &format!("{:.0}", turn));
        ShowWindow(hwnd, SW_SHOW);
        SetFocus(walk);
        SetTimer(hwnd, 1, 100, None);
        let location = if app.area.contains(app.point) {
            format!(
                "{}, facing {}.",
                app.area.location_brief(app.point),
                crate::geo::compass(app.heading)
            )
        } else {
            "Map coverage is loading for your saved position.".into()
        };
        app.announce(&format!("Streetwalk. {}. {}. Explore with the top-row actions, or press Escape for virtual walking. F1 opens the README in your browser. {}",app.area.name,app.speech.status,location));
        let preference = data::read::<Preferences>("preferences.json").ok();
        let use_pc_location = if let Some(preference) = preference {
            preference.start_at_pc_location
        } else {
            let answer = MessageBoxW(
                hwnd,
                wide("Start future Streetwalk sessions at this PC's Windows location? Choose Yes to request location permission and load its map now. Choose No to keep your saved virtual position. You can change this later with Ctrl+L or Ctrl+Shift+L.").as_ptr(),
                wide("Streetwalk starting location").as_ptr(),
                MB_YESNO | MB_ICONQUESTION,
            );
            let enabled = answer == IDYES;
            app.set_pc_location_preference(enabled);
            enabled
        };
        app.start_at_pc_location = use_pc_location;
        if use_pc_location {
            app.request_pc_location();
        }
        if refresh_old_area {
            app.ensure_coverage(true);
        }
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            if msg.message == WM_TIMER {
                app.poll();
                continue;
            }
            if msg.message == WM_KEYDOWN {
                let k = msg.wParam as u16;
                let ctrl = GetKeyState(VK_CONTROL as i32) < 0;
                let shift = GetKeyState(VK_SHIFT as i32) < 0;
                if k == VK_F2 {
                    app.speech.stop();
                    continue;
                }
                if ctrl && (k == 0x46 || k == 0x44) {
                    app.open_search(k == 0x44);
                    continue;
                }
                if ctrl && k == 0x4c {
                    if shift {
                        app.set_pc_location_preference(false);
                        app.announce("Start at PC location is off. Your virtual position will be restored next time.");
                    } else {
                        app.set_pc_location_preference(true);
                        app.request_pc_location();
                    }
                    continue;
                }
                if ctrl && k == 0x49 {
                    app.place_info();
                    continue;
                }
                if k == VK_ESCAPE {
                    app.show_view(View::Explore);
                    SetFocus(walk);
                    app.describe();
                    continue;
                }
                if k == VK_F1 {
                    app.help();
                    continue;
                }
                let focus = GetFocus();
                if focus == results || focus == app.route_list {
                    if k == VK_NEXT || k == VK_PRIOR {
                        app.change_page(if k == VK_NEXT { 1 } else { -1 }, false);
                        continue;
                    }
                    if k == VK_DOWN || k == VK_UP {
                        let selected = SendMessageW(focus, LB_GETCURSEL, 0, 0);
                        let count = SendMessageW(focus, LB_GETCOUNT, 0, 0);
                        if (k == VK_DOWN && selected == count - 1) || (k == VK_UP && selected == 0)
                        {
                            let previous = if focus == app.route_list {
                                app.route_page
                            } else {
                                app.page
                            };
                            app.change_page(if k == VK_DOWN { 1 } else { -1 }, k == VK_UP);
                            let current = if focus == app.route_list {
                                app.route_page
                            } else {
                                app.page
                            };
                            if current != previous {
                                continue;
                            }
                        }
                    }
                    if focus == results && k == 0x47 && !ctrl {
                        app.route_selected();
                        continue;
                    }
                    if focus == results && k == 0x49 && !ctrl {
                        app.place_info();
                        continue;
                    }
                }
                if focus == output && k == VK_TAB {
                    let next = GetNextDlgTabItem(hwnd, output, if shift { 1 } else { 0 });
                    if !next.is_null() {
                        SetFocus(next);
                    }
                    continue;
                }
                if k == VK_RETURN && focus == search {
                    app.search();
                    continue;
                }
                if k == VK_RETURN && focus == results {
                    if ctrl && shift {
                        app.drive_selected();
                    } else if ctrl
                        || (app.destination_mode && app.browse_mode == BrowseMode::Search)
                    {
                        app.route_selected();
                    } else {
                        app.load_selected();
                    }
                    continue;
                }
                if focus == results && app.view == View::Settings && (k == VK_RIGHT || k == VK_LEFT)
                {
                    app.adjust_settings(if k == VK_RIGHT { 1 } else { -1 });
                    continue;
                }
                if k == VK_RETURN && focus == app.route_list {
                    app.key(0x46, false);
                    continue;
                }
                if k == VK_BACK && focus == app.route_list {
                    app.show_view(View::Explore);
                    SetFocus(walk);
                    continue;
                }
                if k == VK_BACK
                    && focus == results
                    && matches!(
                        app.browse_mode,
                        BrowseMode::Places | BrowseMode::AllPlaces | BrowseMode::Explore
                    )
                {
                    if app.browse_mode == BrowseMode::Places {
                        app.show_categories();
                    } else {
                        app.show_view(View::Explore);
                        SetFocus(walk);
                        app.describe();
                    }
                    continue;
                }
                if k == VK_RETURN && focus == heading_input {
                    app.face();
                    continue;
                }
                if k == VK_RETURN && focus == turn_input {
                    app.set_turn();
                    continue;
                }
                if focus == walk && k != VK_TAB && !ctrl && GetKeyState(VK_MENU as i32) >= 0 {
                    if msg.lParam & (1 << 30) == 0 {
                        app.key(k, shift);
                    }
                    continue;
                }
            }
            if msg.message == WM_APP + 1 {
                match (msg.wParam & 0xffff) as i32 {
                    MY_LOCATION => {
                        app.show_view(View::Explore);
                        app.describe();
                        SetFocus(walk);
                    }
                    AROUND_ME => app.explore(false),
                    AHEAD_OF_ME => app.explore(true),
                    NEARBY => app.show_categories(),
                    SEARCH_ACTION => {
                        app.open_search(false);
                    }
                    ROUTE_ACTION => app.show_route(),
                    SAVED_ACTION => app.show_saved(),
                    STREET_PREVIEW => app.show_preview(),
                    BEACON_ACTION => {
                        app.key(0x4B, false);
                        app.show_view(View::Explore);
                        SetFocus(walk);
                    }
                    FIND => app.search(),
                    LOAD => app.load_selected(),
                    ROUTE => app.route_selected(),
                    PLACE_INFO => app.place_info(),
                    DRIVE_TOUR => app.drive_selected(),
                    DRIVE_OPTIONS => app.show_settings(),
                    OPTIONS_NEXT => app.adjust_settings(1),
                    OPTIONS_PREV => app.adjust_settings(-1),
                    WEATHER_ACTION => app.request_weather(true),
                    ROUTE_FOLLOW => app.key(0x46, false),
                    ROUTE_RECALC => app.key(0x47, true),
                    ROUTE_CANCEL => app.key(0x55, false),
                    ROUTE_BEACON => app.key(0x4B, false),
                    PAGE_PREVIOUS => app.change_page(-1, false),
                    PAGE_NEXT => app.change_page(1, false),
                    FACE => app.face(),
                    APPLY => app.set_turn(),
                    RESULTS
                        if (msg.wParam >> 16) as u16 == LBN_SELCHANGE as u16
                            && matches!(
                                app.browse_mode,
                                BrowseMode::Places | BrowseMode::AllPlaces | BrowseMode::Explore
                            )
                            && !app.muted =>
                    {
                        if let Some(choice) =
                            app.selected_choice_index().and_then(|i| app.choices.get(i))
                        {
                            app.cue_point(choice.point);
                        }
                    }
                    FIX_MAP if app.driving_settings.google_mode => {
                        app.announce(
                            "To suggest a correction, open Google Maps and choose Edit the map.",
                        );
                    }
                    FIX_MAP
                        if std::process::Command::new("explorer.exe")
                            .arg("https://www.openstreetmap.org/fixthemap")
                            .spawn()
                            .is_err() =>
                    {
                        app.announce(
                            "Open https://www.openstreetmap.org/fixthemap in your browser.",
                        );
                    }
                    WALK => {
                        app.show_view(View::Explore);
                        SetFocus(walk);
                        app.describe();
                    }
                    _ => {}
                }
                continue;
            }
            if IsDialogMessageW(hwnd, &msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        app.persist();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabling_live_traffic_clears_speed_and_closure_state() {
        let area = Area::demo();
        let route = Route::demo(
            &area,
            area.center.walk(0., 100.),
            "Destination".into(),
            area.center.walk(90., 100.),
        )
        .unwrap();
        let mut drive = Drive::new(route, 50., false, 1., None);
        drive.traffic_kmh = Some(12.);
        drive.lead = Some(LeadTraffic::new(0., 12., 2.));
        drive.paused = true;
        drive.paused_by_closure = true;
        drive.clear_live_traffic();
        assert!(drive.traffic_kmh.is_none());
        assert!(drive.lead.is_none());
        assert!(!drive.paused);

        drive.paused = true;
        drive.clear_live_traffic();
        assert!(drive.paused, "a manual pause must remain in effect");
    }

    #[test]
    fn drive_poi_waits_two_and_a_half_seconds_after_street_speech() {
        let street_spoken = Instant::now();
        assert!(!drive_poi_ready_after_street(
            street_spoken + Duration::from_millis(2_499),
            street_spoken
        ));
        assert!(drive_poi_ready_after_street(
            street_spoken + Duration::from_millis(2_500),
            street_spoken
        ));
    }

    #[test]
    fn drive_announces_advance_and_immediate_turn() {
        let start = Point {
            lat: 47.,
            lon: -122.,
        };
        let turn = start.walk(0., 120.);
        let destination = turn.walk(90., 120.);
        let route = Route::new(
            "Test".into(),
            destination,
            vec![start, turn, destination],
            vec![
                crate::navigation::Maneuver {
                    at: 0.,
                    text: "Head onto Main Street".into(),
                    bearing: 0.,
                },
                crate::navigation::Maneuver {
                    at: 120.,
                    text: "Turn left onto Pine Street".into(),
                    bearing: 270.,
                },
                crate::navigation::Maneuver {
                    at: 240.,
                    text: "Arrive at your destination".into(),
                    bearing: 90.,
                },
            ],
        )
        .unwrap();
        let (preview, index, previewed) =
            drive_maneuver_callout(&route, 0., 50., 1, false).unwrap();
        assert_eq!(preview, "In 120 metres, turn left onto Pine Street.");
        assert_eq!(index, 1);
        let (now, index, _) = drive_maneuver_callout(&route, 116., 30., index, previewed).unwrap();
        assert_eq!(now, "Turning left onto Pine Street.");
        assert_eq!(index, 2);
        assert!(drive_maneuver_callout(&route, 230., 30., index, false).is_none());
    }
    #[test]
    fn periodic_drive_address_uses_nearby_numbered_address() {
        let mut area = Area::demo();
        let point = area.center;
        area.places.push(Place {
            name: "123 Main Street".into(),
            kind: "address".into(),
            street: "Main Street".into(),
            point,
            group: String::new(),
            wikipedia: String::new(),
            wikidata: String::new(),
            description: String::new(),
        });
        let address = AddressContext {
            city: "Seattle".into(),
            ..Default::default()
        };
        assert_eq!(
            drive_address_phrase(&area, point, &address).unwrap(),
            "Near 123 Main Street, Seattle."
        );
    }

    #[test]
    fn drive_can_call_out_routine_cafe_ahead() {
        let mut area = Area::demo();
        let start = area.center;
        let destination = start.walk(0., 500.);
        area.places = vec![Place {
            name: "Corner Coffee".into(),
            kind: "cafe".into(),
            street: String::new(),
            point: start.walk(0., 100.).walk(90., 25.),
            group: "Food and drink".into(),
            wikipedia: String::new(),
            wikidata: String::new(),
            description: String::new(),
        }];
        let route = Route::new(
            "Test".into(),
            destination,
            vec![start, destination],
            vec![crate::navigation::Maneuver {
                at: 0.,
                text: "Continue".into(),
                bearing: 0.,
            }],
        )
        .unwrap();
        let mut mentioned = HashSet::new();
        assert_eq!(
            drive_poi_candidate(&area, &route, 0., 70., &mentioned),
            Some(0)
        );
        mentioned.insert(area.places[0].key());
        assert_eq!(
            drive_poi_candidate(&area, &route, 0., 70., &mentioned),
            None
        );
        assert_eq!(
            area.callout_place(0, start, 0.),
            "Corner Coffee, 12 o'clock"
        );
    }

    #[test]
    fn driving_options_and_speed_changes() {
        let mut settings = DrivingSettings::default();
        assert!(settings.valid());
        settings.adjust(0, -1);
        assert_eq!(settings.route_preference, HighwayPreference::AvoidMotorways);
        settings.adjust(1, 1);
        assert_eq!(settings.signal_wait_seconds, 5);
        settings.adjust(3, 1);
        assert_eq!(settings.fallback.built_up, 40.);
        let driver = DriverProfile::from_seed(0);
        assert!(approach_speed(0., 50., 1., 0., driver).0 > 0.);
        assert!(approach_speed(50., 0., 1., 0., driver).0 < 50.);
        settings.adjust(8, 1);
        assert!(!settings.ev_audio);
        settings.adjust(9, -1);
        assert_eq!(settings.ev_volume, 60);
    }
    #[test]
    fn ordinary_launch_is_gentler_than_leaf_full_throttle() {
        let (mut speed, mut acceleration) = (0., 0.);
        let mut reached_60_mph = None;
        for tick in 0..2_000 {
            (speed, acceleration) =
                approach_speed(speed, 100., 0.02, acceleration, DriverProfile::from_seed(0));
            if speed >= 96.56 {
                reached_60_mph = Some((tick + 1) as f64 * 0.02);
                break;
            }
        }
        assert!(reached_60_mph.is_some_and(|seconds| (15. ..40.).contains(&seconds)));
        assert!(approach_speed(0., 50., 0.02, 0., DriverProfile::from_seed(0)).1 < 0.1);
        let (next, braking) = approach_speed(50., 0., 0.02, 0., DriverProfile::from_seed(0));
        assert!(next > 49. && (-0.1..0.).contains(&braking));

        let mut full_speed = 0.;
        let mut full_time = 0.;
        while full_speed < 100. && full_time < 20. {
            full_speed += leaf_available_acceleration(full_speed) * 0.02 * 3.6;
            full_time += 0.02;
        }
        assert!((11. ..12.).contains(&full_time));
    }
    #[test]
    fn sampled_drivers_vary_coherently_and_keep_turns_bounded() {
        let profiles: Vec<_> = (0..128).map(DriverProfile::from_seed).collect();
        let min_accel = profiles
            .iter()
            .map(|p| p.acceleration)
            .fold(f64::INFINITY, f64::min);
        let max_accel = profiles.iter().map(|p| p.acceleration).fold(0., f64::max);
        assert!(max_accel - min_accel > 0.3);
        for profile in profiles {
            assert!((0.8..=2.0).contains(&profile.acceleration));
            assert!((0.75..=1.3).contains(&profile.corner_variation(1)));
            assert!((0.4..=1.4).contains(&profile.reaction_seconds));
            let (speed, _) = approach_speed(40., 45., 0.5, 0., profile);
            assert!(speed > 40. && speed < 45.);
        }
        let reserved = DriverProfile::from_seed(9);
        assert_eq!(reserved.corner_variation(4), reserved.corner_variation(4));
        assert_ne!(reserved.corner_variation(4), reserved.corner_variation(5));
    }
    #[test]
    fn route_geometry_slows_for_bend_without_named_maneuver() {
        let start = Point {
            lat: 47.,
            lon: -122.,
        };
        let points = vec![
            start,
            start.walk(0., 250.),
            start.walk(0., 250.).walk(90., 100.),
            start.walk(0., 250.).walk(90., 250.),
        ];
        let route = Route::new(
            "Bend".into(),
            *points.last().unwrap(),
            points,
            vec![crate::navigation::Maneuver {
                at: 0.,
                text: "Head north".into(),
                bearing: 0.,
            }],
        )
        .unwrap();
        let driver = DriverProfile::from_seed(11);
        assert!(bend_speed_cap(&route, 30., driver) > 70.);
        assert!(bend_speed_cap(&route, 220., driver) < 55.);
    }
    #[test]
    fn virtual_lead_car_changes_speed_and_preserves_following_gap() {
        let mut lead = LeadTraffic::new(0., 25., 2.);
        let mut ego = 0.;
        let mut low = f64::INFINITY;
        let mut high: f64 = 0.;
        for _ in 0..400 {
            let cap = lead.advance(0.1, ego, 25., 18., 2.);
            low = low.min(cap);
            high = high.max(cap);
            ego += cap.min(25.) / 36.;
            assert!(lead.at >= ego - 2.);
        }
        assert!(high - low > 3.);
    }
    #[test]
    fn elevation_grade_is_smoothed_and_hills_reduce_available_acceleration() {
        let samples = vec![(0., 100.), (100., 106.), (200., 112.), (300., 118.)];
        assert!((route_grade(&samples, 25.) - 0.06).abs() < 0.001);
        assert_eq!(route_grade(&samples, 300.), 0.);
        let driver = DriverProfile::from_seed(3);
        let flat = approach_speed_on_grade(100., 120., 1., 0., driver, 0.).0;
        let uphill = approach_speed_on_grade(100., 120., 1., 0., driver, 0.08).0;
        assert!(uphill < flat);
    }
    #[test]
    fn built_up_traffic_and_reported_slowdown_raise_audible_car_rate() {
        let flow = TrafficFlow {
            current_kmh: 20.,
            free_kmh: 50.,
            confidence: 0.9,
            closed: false,
            coordinates: vec![],
        };
        let rural = estimated_traffic_activity(0.1, None);
        let urban = estimated_traffic_activity(1., None);
        let slow_urban = estimated_traffic_activity(1., Some(&flow));
        assert!(urban > rural * 3.);
        assert!(slow_urban > urban + 0.15);
        assert!(3. + slow_urban * 24. > 20.);
    }

    #[test]
    fn drive_brakes_for_corner_but_not_a_long_straight() {
        let start = Point {
            lat: 47.,
            lon: -122.,
        };
        let corner = start.walk(0., 500.);
        let end = corner.walk(90., 500.);
        let route = Route::new(
            "End".into(),
            end,
            vec![start, corner, end],
            vec![
                crate::navigation::Maneuver {
                    at: 0.,
                    text: "Head north".into(),
                    bearing: 0.,
                },
                crate::navigation::Maneuver {
                    at: 500.,
                    text: "Turn right".into(),
                    bearing: 90.,
                },
                crate::navigation::Maneuver {
                    at: 1000.,
                    text: "Arrive".into(),
                    bearing: 90.,
                },
            ],
        )
        .unwrap();
        let driver = DriverProfile::from_seed(0);
        assert!(maneuver_speed_cap(&route, 0., driver) > 100.);
        assert!(maneuver_speed_cap(&route, 470., driver) < 55.);
        assert!(maneuver_speed_cap(&route, 490., driver) < 35.);
        assert_eq!(turn_indicator_side(&route, 200., 50.), 0);
        assert_eq!(turn_indicator_side(&route, 440., 50.), 1);
        assert_eq!(turn_indicator_side(&route, 510., 50.), 0);
    }

    #[test]
    fn pages_cover_every_result_without_overlap() {
        assert_eq!(page_bounds(0, 0), (0, 0, 0, 1));
        assert_eq!(page_bounds(23, 0), (0, 0, 10, 3));
        assert_eq!(page_bounds(23, 1), (1, 10, 20, 3));
        assert_eq!(page_bounds(23, 2), (2, 20, 23, 3));
        assert_eq!(page_bounds(23, 99), (2, 20, 23, 3));
        assert_eq!(page_selection_index(2, 2, 23), Some(22));
        assert_eq!(page_selection_index(2, 3, 23), None);
    }
    #[test]
    fn tour_context_skips_routine_places() {
        let mut place = Area::demo().places[0].clone();
        place.group = "Sights".into();
        place.wikipedia = "en:Example".into();
        assert!(notable(&place));
        place.group = "Shopping".into();
        place.kind = "supermarket".into();
        assert!(!notable(&place));
    }
    #[test]
    fn p_list_excludes_addresses_and_l_speaks_separate_items() {
        let mut area = Area::demo();
        area.places.push(crate::map::Place {
            name: "123 Main Street".into(),
            kind: "address".into(),
            street: String::new(),
            point: area.center,
            group: String::new(),
            wikipedia: String::new(),
            wikidata: String::new(),
            description: String::new(),
        });
        let choices = all_poi_choices(&area, area.center, 0.);
        assert_eq!(choices.len(), area.places.len() - 1);
        assert!(!choices.iter().any(|c| c.name.contains("123 Main")));
        assert!(
            choices
                .windows(2)
                .all(|pair| area.center.distance(pair[0].point)
                    <= area.center.distance(pair[1].point))
        );
        assert_eq!(
            spoken_places(&["Cafe".into(), "Library".into()]),
            "Cafe. Library."
        );
    }

    #[test]
    fn walking_selection_snap_undo_and_step_limits() {
        unsafe {
            // Exercise the controller against a real, hidden native text control.
            // No input injection, foreground changes, speech or network access.
            let output = CreateWindowExW(
                0,
                wide("STATIC").as_ptr(),
                wide("").as_ptr(),
                0,
                0,
                0,
                100,
                100,
                null_mut(),
                null_mut(),
                GetModuleHandleW(null_mut()),
                null_mut(),
            );
            assert!(!output.is_null());
            let (tx, _jobs) = mpsc::channel();
            let map_tx = tx.clone();
            let (_send, rx) = mpsc::channel();
            let area = Area::demo();
            let home = area.center;
            let mut app = App {
                map_attribution: std::ptr::null_mut(),
                route_attribution: std::ptr::null_mut(),
                hwnd: null_mut(),
                view_title: null_mut(),
                search: null_mut(),
                results: null_mut(),
                walk: null_mut(),
                output,
                route_title: null_mut(),
                route_list: null_mut(),
                page_previous: null_mut(),
                page_next: null_mut(),
                heading_input: null_mut(),
                turn_input: null_mut(),
                area,
                point: home,
                heading: 0.,
                step: 2,
                history: vec![],
                choices: vec![],
                page: 0,
                route_page: 0,
                preview_options: vec![],
                browse_mode: BrowseMode::Search,
                categories: vec![],
                speech: Speech::new(),
                audio: Audio::new(),
                muted: true,
                busy: false,
                tx,
                map_tx,
                rx,
                location_tx: _send.clone(),
                start_at_pc_location: false,
                announcer: Announcer::default(),
                route: None,
                drive: None,
                drive_generation: 0,
                traffic_generation: 0,
                driving_settings: DrivingSettings::default(),
                weather_pending: false,
                traffic_pending: false,
                last_traffic_request: None,
                last_traffic: None,
                traffic_error_reported: false,
                incidents_pending: false,
                last_incidents_request: None,
                incidents: vec![],
                incidents_error_reported: false,
                drive_address_pending: false,
                last_drive_address_request: None,
                pending_drive_address: None,
                google_walk_pending: false,
                last_google_walk_request: None,
                last_weather_request: None,
                last_weather: None,
                turn: 15.,
                destination_mode: false,
                spare: vec![],
                map_generation: 0,
                route_generation: 0,
                cover_pending: false,
                last_cover: None,
                map_error: false,
                last_walk: None,
                view: View::Explore,
                search_controls: vec![],
                results_controls: vec![],
                walking_controls: vec![],
                route_controls: vec![],
                settings_controls: vec![],
                announcements: RefCell::new(VecDeque::new()),
            };
            app.key(VK_RIGHT, false);
            assert!((home.distance(app.point) - 10.).abs() < 0.01);
            assert_eq!(app.heading, 90.);
            let mut value = [0u16; 2048];
            let n = GetWindowTextW(output, value.as_mut_ptr(), 2048);
            assert!(String::from_utf16_lossy(&value[..n as usize]).contains("corner"));
            for _ in 0..10 {
                app.key(VK_ADD, false);
            }
            assert_eq!(app.step, 5);
            for _ in 0..10 {
                app.key(VK_SUBTRACT, false);
            }
            assert_eq!(app.step, 0);
            let destination = app.area.places[0].point;
            let before_jump = app.point;
            app.move_to(destination, app.heading);
            assert_eq!(app.point, destination);
            app.key(VK_BACK, false);
            assert_eq!(app.point, before_jump);
            app.move_to(home.walk(45., 40.), 45.);
            app.key(0x53, false);
            assert!(app.area.nearest_road(app.point).unwrap().2 < 0.001);
            let before_edge = app.point;
            app.move_to(home.walk(0., 2000.), 0.);
            assert_eq!(app.point, before_edge);
            // Repeated movement on the same street does not replace the transcript.
            app.move_to(home.walk(0., 250.), 0.);
            let n = GetWindowTextW(output, value.as_mut_ptr(), 2048);
            let previous = String::from_utf16_lossy(&value[..n as usize]);
            app.key(VK_UP, false);
            let n = GetWindowTextW(output, value.as_mut_ptr(), 2048);
            assert_eq!(String::from_utf16_lossy(&value[..n as usize]), previous);
            app.heading = 350.;
            app.key(0x45, false);
            assert_eq!(app.heading, 5.);
            app.key(0x51, true);
            assert_eq!(app.heading, 4.);
            let before = app.point;
            app.key(0x57, false);
            assert!((before.bearing(app.point) - 4.).abs() < 0.01);
            assert_eq!(app.heading, 4.);

            // Automatic coverage permits crossing map boundaries and never teleports on completion.
            app.area.name = "Real fixture".into();
            let beyond = home.walk(0., 2000.);
            app.move_to(beyond, 0.);
            assert_eq!(app.point, beyond);
            assert!(app.cover_pending);
            assert!(!app.busy);
            assert!(matches!(_jobs.try_recv().unwrap(), Job::Cover(_, 0)));
            let history_len = app.history.len();
            let mut cover = Area::demo();
            cover.name = "Downloaded fixture".into();
            cover.center = beyond.walk(0., 100.);
            _send.send(Reply::Cover(Ok(cover.clone()), 0)).unwrap();
            app.poll();
            assert_eq!(app.point, beyond);
            assert_eq!(app.heading, 0.);
            assert_eq!(app.history.len(), history_len);
            assert_eq!(app.area.name, "Downloaded fixture");
            app.map_generation = 1;
            _send.send(Reply::Cover(Ok(Area::demo()), 0)).unwrap();
            app.poll();
            assert_eq!(
                app.area.name, "Downloaded fixture",
                "Stale responses must not restore an old map"
            );

            // A cancelled pending route cannot reappear when the worker finishes.
            let route = Route::demo(
                &Area::demo(),
                home.walk(0., 100.),
                "Destination".into(),
                home.walk(90., 100.),
            )
            .unwrap();
            app.route_generation = 2;
            app.key(0x55, false);
            _send.send(Reply::Route(Ok(route), app.point, 2)).unwrap();
            app.poll();
            assert!(app.route.is_none());
            DestroyWindow(output);
        }
    }
}
