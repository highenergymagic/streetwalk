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
            .and_then(|dir| std::fs::read_to_string(dir.join("USERGUIDE.md")).ok())
            .unwrap_or_else(|| include_str!("../USERGUIDE.md").to_owned());
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
        let path = data::directory().join("USERGUIDE.html");
        let result =
            std::fs::create_dir_all(data::directory()).and_then(|_| std::fs::write(&path, page));
        if let Err(e) = result {
            self.announce(&format!("Could not open the user guide: {e}"));
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
                self.announce("Could not open the user guide in your default browser.");
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
}
mod browse;
mod drive;
mod drive_model;
mod keyboard;
mod replies;
mod window;
use browse::*;
use drive_model::*;
pub use window::run;

#[cfg(test)]
mod tests;
