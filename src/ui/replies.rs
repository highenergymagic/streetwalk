use super::*;

impl App {
    pub(super) fn poll(&mut self) {
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
}
