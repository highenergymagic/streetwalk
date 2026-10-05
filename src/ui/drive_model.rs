use super::*;

pub(super) struct Drive {
    pub(super) route: Route,
    pub(super) progress: f64,
    pub(super) last_tick: Instant,
    pub(super) last_callout: Instant,
    pub(super) last_poi_scan: Instant,
    pub(super) last_context_callout: Instant,
    pub(super) mentioned: HashSet<String>,
    pub(super) paused: bool,
    pub(super) paused_by_closure: bool,
    pub(super) speed_kmh: f64,
    pub(super) traffic_kmh: Option<f64>,
    pub(super) lead: Option<LeadTraffic>,
    pub(super) actual_speed_kmh: f64,
    pub(super) actual_acceleration_mps2: f64,
    pub(super) driver: DriverProfile,
    pub(super) departure_delay: f64,
    pub(super) texture: f64,
    pub(super) tagged_speed: bool,
    pub(super) last_speed_callout: Instant,
    pub(super) last_speed_check: Instant,
    pub(super) stop: Option<TrafficStop>,
    pub(super) seen_road_events: HashSet<String>,
    pub(super) road_event_counter: u32,
    pub(super) road_event_kind: u8,
    pub(super) elevation_samples: Vec<(f64, f64)>,
    pub(super) elevation_pending: bool,
    pub(super) elevation_until: f64,
    pub(super) seen_signals: HashSet<String>,
    pub(super) mentioned_incidents: HashSet<String>,
    pub(super) last_incident_callout: Instant,
    pub(super) next_maneuver: usize,
    pub(super) maneuver_previewed: bool,
    pub(super) last_street_context: Option<String>,
    pub(super) last_street_announcement: Instant,
    pub(super) last_street_speech: Instant,
    pub(super) last_guidance_announcement: Instant,
    pub(super) last_guidance_progress: f64,
}
impl Drive {
    pub(super) fn new(
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
    pub(super) fn clear_live_traffic(&mut self) {
        self.traffic_kmh = None;
        self.lead = None;
        if self.paused_by_closure {
            self.paused = false;
            self.paused_by_closure = false;
            self.last_tick = Instant::now();
        }
    }
}
pub(super) fn route_grade(samples: &[(f64, f64)], progress: f64) -> f64 {
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
pub(super) struct DriverProfile {
    pub(super) acceleration: f64,
    pub(super) braking: f64,
    pub(super) jerk: f64,
    pub(super) corner_factor: f64,
    pub(super) cruise_factor: f64,
    pub(super) reaction_seconds: f64,
    pub(super) coast_deceleration: f64,
    pub(super) headway_seconds: f64,
}
impl DriverProfile {
    pub(super) fn sampled() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        Self::from_seed(seed)
    }
    pub(super) fn from_seed(mut seed: u64) -> Self {
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
    pub(super) fn corner_variation(self, index: usize) -> f64 {
        let hash = (index as u64).wrapping_mul(0x9e3779b97f4a7c15);
        let fraction = ((hash ^ (hash >> 32)) & 0xffff) as f64 / 65535.;
        self.corner_factor * (0.94 + fraction * 0.12)
    }
}
pub(super) struct LeadTraffic {
    pub(super) at: f64,
    pub(super) speed_kmh: f64,
    pub(super) phase: f64,
}
impl LeadTraffic {
    pub(super) fn new(progress: f64, ego_kmh: f64, headway: f64) -> Self {
        Self {
            at: progress + 12. + ego_kmh / 3.6 * headway,
            speed_kmh: ego_kmh,
            phase: 0.,
        }
    }
    pub(super) fn advance(
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
pub(super) fn drive_poi_ready_after_street(now: Instant, last_street_speech: Instant) -> bool {
    now.duration_since(last_street_speech) >= Duration::from_millis(2_500)
}
pub(super) struct TrafficStop {
    pub(super) at: f64,
    pub(super) remaining: f64,
    pub(super) reached: bool,
}
pub(super) fn leaf_available_acceleration(current: f64) -> f64 {
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
pub(super) fn approach_speed(
    current: f64,
    target: f64,
    elapsed: f64,
    previous_acceleration: f64,
    driver: DriverProfile,
) -> (f64, f64) {
    approach_speed_on_grade(current, target, elapsed, previous_acceleration, driver, 0.)
}
pub(super) fn approach_speed_on_grade(
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
pub(super) fn estimated_traffic_activity(urbanity: f64, flow: Option<&TrafficFlow>) -> f32 {
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
pub(super) fn turn_indicator_side(route: &Route, progress: f64, speed_kmh: f64) -> i8 {
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

pub(super) fn maneuver_speed_cap(route: &Route, progress: f64, driver: DriverProfile) -> f64 {
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
pub(super) fn bend_speed_cap(route: &Route, progress: f64, driver: DriverProfile) -> f64 {
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
pub(super) fn notable(place: &Place) -> bool {
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
pub(super) fn drive_poi_candidate(
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
pub(super) fn drive_maneuver_callout(
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
pub(super) fn drive_address_phrase(
    area: &Area,
    point: Point,
    address: &AddressContext,
) -> Option<String> {
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
