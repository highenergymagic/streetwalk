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
    let (preview, index, previewed) = drive_maneuver_callout(&route, 0., 50., 1, false).unwrap();
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
    assert!(choices
        .windows(2)
        .all(|pair| area.center.distance(pair[0].point) <= area.center.distance(pair[1].point)));
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
