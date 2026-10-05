use super::*;

impl App {
    pub(super) fn show_settings(&mut self) {
        self.browse_mode = BrowseMode::Settings;
        self.page = 0;
        self.show_view(View::Settings);
        self.render_results_page();
        unsafe { SetFocus(self.results) };
        self.announce("Map and driving options. Choose a setting, then press Enter or Right to increase it, or Left to decrease it. Escape returns to Explore.");
    }
    pub(super) fn adjust_settings(&mut self, direction: i32) {
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
    pub(super) fn request_weather(&mut self, spoken: bool) {
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
    pub(super) fn request_traffic(&mut self, force: bool) {
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
    pub(super) fn request_incidents(&mut self, force: bool) {
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
    pub(super) fn request_drive_address(&mut self, force: bool) {
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
    pub(super) fn request_elevation(&mut self) {
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
    pub(super) fn drive_tick(&mut self) {
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
}
