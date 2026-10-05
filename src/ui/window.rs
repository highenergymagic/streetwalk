use super::*;

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
        app.announce(&format!("Streetwalk. {}. {}. Explore with the top-row actions, or press Escape for virtual walking. F1 opens the user guide in your browser. {}",app.area.name,app.speech.status,location));
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
