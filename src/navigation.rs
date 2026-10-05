use crate::{
    geo::{compass, project, Point},
    map::Area,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

pub fn turn_delta(before: f64, after: f64) -> f64 {
    (after - before + 180.).rem_euclid(360.) - 180.
}
pub fn turn_words(before: f64, after: f64) -> String {
    let delta = turn_delta(before, after);
    if delta.abs() < 10. {
        "Continue straight".into()
    } else if delta.abs() > 170. {
        "Turn around 180 degrees".into()
    } else {
        format!(
            "Turn {} {:.0} degrees",
            if delta > 0. { "right" } else { "left" },
            delta.abs()
        )
    }
}
fn driving_instruction(kind: &str, modifier: &str, street: &str, exit: Option<u64>) -> String {
    let side = if modifier.contains("left") {
        "left"
    } else if modifier.contains("right") {
        "right"
    } else {
        "straight"
    };
    match kind {
        "depart" => format!("Head onto {street}"),
        "arrive" => "Arrive at your destination".into(),
        "roundabout" | "rotary" | "exit roundabout" => match exit {
            Some(number) => format!("At the roundabout, take exit {number} onto {street}"),
            None => format!("Leave the roundabout onto {street}"),
        },
        "continue" => {
            if side == "straight" {
                format!("Continue straight to stay on {street}")
            } else {
                format!("Keep {side} to stay on {street}")
            }
        }
        "new name" => format!("Continue straight onto {street}"),
        "merge" => format!("Merge {side} onto {street}"),
        "fork" => format!("Keep {side} onto {street}"),
        "on ramp" | "off ramp" => format!("Take the ramp {side} onto {street}"),
        "turn" | "end of road" if side != "straight" => format!("Turn {side} onto {street}"),
        "notification" => format!("Continue on {street}"),
        _ => format!("Continue straight onto {street}"),
    }
}

#[derive(Default)]
pub struct Announcer {
    context: Option<String>,
    places: HashMap<String, Point>,
    last_orientation: Option<Point>,
    last_orientation_at: Option<Instant>,
    last_ambient_at: Option<Instant>,
}
impl Announcer {
    pub fn reset(&mut self, area: &Area, p: Point) {
        self.context = area.context(p).map(|c| c.0);
        self.last_orientation = Some(p);
        self.last_orientation_at = Some(Instant::now());
        self.last_ambient_at = None;
        self.places.clear();
        for i in area.nearby(p).into_iter().take(2).filter(|i| {
            let place = &area.places[*i];
            place.kind != "address"
                && p.distance(place.point) < if place.kind == "crossing" { 18. } else { 150. }
        }) {
            let place = &area.places[i];
            self.places.insert(place.key(), place.point);
        }
    }
    pub fn update(&mut self, area: &Area, p: Point, heading: f64) -> Vec<String> {
        self.update_at(area, p, heading, Instant::now())
    }
    fn update_at(&mut self, area: &Area, p: Point, heading: f64, now: Instant) -> Vec<String> {
        self.places.retain(|_, q| p.distance(*q) < 140.);
        let mut events = vec![];
        let context = area.context(p);
        let retain_street = context.is_none()
            && area.nearest_road(p).is_some_and(|(r, _, d)| {
                d < 40. && self.context.as_ref() == Some(&format!("street:{}", r.name))
            });
        if !retain_street && context.as_ref().map(|c| &c.0) != self.context.as_ref() {
            if let Some((_, text)) = &context {
                events.push(text.clone());
            }
            self.context = context.map(|c| c.0);
        }
        let mut nearby: Vec<_> = area
            .nearby(p)
            .into_iter()
            .filter(|i| {
                let place = &area.places[*i];
                place.kind != "address"
                    && p.distance(place.point) < if place.kind == "crossing" { 18. } else { 60. }
            })
            .collect();
        nearby.sort_by_key(|i| area.places[*i].kind != "crossing");
        let ambient_ready = self
            .last_ambient_at
            .is_none_or(|last| now.duration_since(last) >= Duration::from_secs(6));
        for i in nearby.into_iter().take(2) {
            let place = &area.places[i];
            if !self.places.contains_key(&place.key())
                && (place.kind == "crossing" || ambient_ready)
            {
                events.push(area.callout_place(i, p, heading));
                self.places.insert(place.key(), place.point);
                if place.kind != "crossing" {
                    self.last_ambient_at = Some(now);
                    break;
                }
            }
        }
        if self
            .last_orientation
            .is_none_or(|last| last.distance(p) >= 250.)
            && self
                .last_orientation_at
                .is_none_or(|last| now.duration_since(last) >= Duration::from_secs(20))
        {
            if events.is_empty() {
                events.push(format!("Continuing on {}.", area.location_brief(p)));
            }
            self.last_orientation = Some(p);
            self.last_orientation_at = Some(now);
        }
        events
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Maneuver {
    pub at: f64,
    pub text: String,
    pub bearing: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Route {
    pub name: String,
    pub destination: Point,
    pub points: Vec<Point>,
    pub distances: Vec<f64>,
    pub maneuvers: Vec<Maneuver>,
    #[serde(default)]
    pub cruise_speeds: Vec<(f64, f64)>,
}
impl Route {
    pub fn cruise_speed(&self, progress: f64) -> Option<f64> {
        self.cruise_speeds
            .iter()
            .rev()
            .find(|(at, _)| *at <= progress)
            .map(|(_, speed)| *speed)
    }
    pub fn length(&self) -> f64 {
        *self.distances.last().unwrap_or(&0.)
    }
    pub fn validate(self) -> Result<Self, String> {
        let Self {
            name,
            destination,
            points,
            maneuvers,
            cruise_speeds,
            ..
        } = self;
        let mut route = Self::new(name, destination, points, maneuvers)?;
        if cruise_speeds.iter().any(|(at, speed)| {
            !at.is_finite()
                || *at < 0.
                || *at > route.length()
                || !speed.is_finite()
                || !(0. ..=160.).contains(speed)
        }) || cruise_speeds.windows(2).any(|pair| pair[0].0 > pair[1].0)
        {
            return Err("Invalid route speed estimates".into());
        }
        route.cruise_speeds = cruise_speeds;
        Ok(route)
    }
    pub fn from_osrm(name: String, destination: Point, json: &str) -> Result<Self, String> {
        Self::from_osrm_mode(name, destination, json, false)
    }
    pub fn from_osrm_driving(name: String, destination: Point, json: &str) -> Result<Self, String> {
        Self::from_osrm_mode(name, destination, json, true)
    }
    fn from_osrm_mode(
        name: String,
        destination: Point,
        json: &str,
        driving: bool,
    ) -> Result<Self, String> {
        let v: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
        if v["code"] != "Ok" {
            return Err(format!(
                "No walking route: {}",
                v["message"]
                    .as_str()
                    .or(v["code"].as_str())
                    .unwrap_or("invalid response")
            ));
        }
        let r = v["routes"].get(0).ok_or("No walking route returned")?;
        let mut points = vec![];
        let mut maneuvers = vec![];
        let mut distance = 0.;
        for leg in r["legs"].as_array().ok_or("Route legs missing")? {
            for step in leg["steps"]
                .as_array()
                .ok_or("Route instructions missing")?
            {
                let m = &step["maneuver"];
                let before = m["bearing_before"].as_f64().unwrap_or(0.);
                let after = m["bearing_after"].as_f64().unwrap_or(0.);
                let kind = m["type"].as_str().unwrap_or("turn");
                let street = step["name"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .unwrap_or(if driving { "the road" } else { "the path" });
                let instruction = if driving {
                    let delta = turn_delta(before, after);
                    let modifier = m["modifier"].as_str().unwrap_or(if delta > 30. {
                        "right"
                    } else if delta < -30. {
                        "left"
                    } else {
                        "straight"
                    });
                    driving_instruction(kind, modifier, street, m["exit"].as_u64())
                } else {
                    match kind {
                    "depart"=>format!("Head {} on {street}, bearing {after:.0} degrees",compass(after)),
                    "arrive"=>"Reach the end of the walking route".into(),
                    "roundabout"|"rotary"=>format!("Enter the roundabout; take exit {} onto {street}, bearing {after:.0} degrees",m["exit"].as_u64().map(|x|x.to_string()).unwrap_or_else(||"shown by the route".into())),
                    _=>format!("{} onto {street}, bearing {after:.0} degrees",turn_words(before,after)),
                }
                };
                let coordinates = step["geometry"]["coordinates"]
                    .as_array()
                    .ok_or("Route geometry missing")?;
                let at = distance;
                for c in coordinates {
                    let p = Point {
                        lon: c[0].as_f64().ok_or("Invalid route longitude")?,
                        lat: c[1].as_f64().ok_or("Invalid route latitude")?,
                    };
                    if !p.valid() {
                        return Err("Invalid route coordinates".into());
                    }
                    if let Some(last) = points.last().copied() {
                        let last: Point = last;
                        let d = last.distance(p);
                        if d < 0.01 {
                            continue;
                        }
                        distance += d;
                    }
                    points.push(p);
                }
                maneuvers.push(Maneuver {
                    at,
                    text: instruction,
                    bearing: after,
                });
            }
        }
        Self::new(name, destination, points, maneuvers)
    }
    pub fn new(
        name: String,
        destination: Point,
        points: Vec<Point>,
        maneuvers: Vec<Maneuver>,
    ) -> Result<Self, String> {
        if points.len() < 2 || !destination.valid() || points.iter().any(|p| !p.valid()) {
            return Err("The route has no usable walking geometry".into());
        }
        let mut distances = vec![0.];
        for s in points.windows(2) {
            distances.push(distances.last().unwrap() + s[0].distance(s[1]));
        }
        let length = *distances.last().unwrap();
        if length < 0.01
            || maneuvers.is_empty()
            || maneuvers.iter().any(|m| {
                !m.at.is_finite() || !m.bearing.is_finite() || m.at < 0. || m.at > length + 1.
            })
            || maneuvers.windows(2).any(|m| m[0].at > m[1].at)
        {
            return Err("Invalid route instructions or distance".into());
        }
        Ok(Self {
            name,
            destination,
            points,
            distances,
            maneuvers,
            cruise_speeds: vec![],
        })
    }
    pub fn position(&self, at: f64) -> (Point, f64) {
        let at = at.clamp(0., self.length());
        let i = self
            .distances
            .partition_point(|d| *d <= at)
            .saturating_sub(1)
            .min(self.points.len() - 2);
        let a = self.points[i];
        let b = self.points[i + 1];
        let h = a.bearing(b);
        if at >= self.length() {
            (b, h)
        } else {
            (a.walk(h, at - self.distances[i]), h)
        }
    }
    pub fn locate(&self, p: Point, prior: f64) -> (f64, f64) {
        self.points
            .windows(2)
            .enumerate()
            .map(|(i, s)| {
                let q = project(p, s[0], s[1]);
                let at = self.distances[i] + s[0].distance(q);
                (at, p.distance(q))
            })
            .min_by(|a, b| {
                // At self-crossings prefer the segment near current progress.
                let score = |x: &(f64, f64)| x.1 + ((x.0 - prior).abs() * 0.002).min(5.);
                score(a).total_cmp(&score(b))
            })
            .unwrap_or((0., f64::INFINITY))
    }
    pub fn locate_ahead(&self, p: Point, from: f64, range: f64) -> (f64, f64) {
        let begin = self
            .distances
            .partition_point(|d| *d < from)
            .saturating_sub(1);
        let end = self
            .distances
            .partition_point(|d| *d <= from + range)
            .min(self.points.len() - 1);
        (begin..end)
            .map(|i| {
                let q = project(p, self.points[i], self.points[i + 1]);
                (
                    self.distances[i] + self.points[i].distance(q),
                    p.distance(q),
                )
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap_or((from, f64::INFINITY))
    }
    pub fn summary(&self) -> String {
        let mut lines=vec![format!("Walking route to {}: {:.0} metres. Destination is {:.0} metres from the mapped route endpoint.",self.name,self.length(),self.points.last().unwrap().distance(self.destination))];
        for m in &self.maneuvers {
            lines.push(format!("At {:.0} metres: {}.", m.at, m.text));
        }
        lines.join("\r\n")
    }
    // The fixture contains two streets joined at its center. Never send fictional places to a service.
    pub fn demo(
        area: &Area,
        start: Point,
        name: String,
        destination: Point,
    ) -> Result<Self, String> {
        if !area.contains(destination) {
            return Err(
                "Choose a destination inside the fictional demo, or load a real place first."
                    .into(),
            );
        }
        let (a, s, _) = area.nearest_road(start).ok_or("No demo street")?;
        let (b, e, _) = area.nearest_road(destination).ok_or("No demo street")?;
        let mut points = vec![s];
        if a.name != b.name && s.distance(area.center) > 0.1 {
            points.push(area.center);
        }
        if points.last().unwrap().distance(e) > 0.1 {
            points.push(e);
        }
        if points.len() < 2 {
            return Err("You are already at the destination's street access point.".into());
        }
        let first = points[0].bearing(points[1]);
        let mut maneuvers = vec![Maneuver {
            at: 0.,
            text: format!(
                "Head {} on {}, bearing {first:.0} degrees",
                compass(first),
                a.name
            ),
            bearing: first,
        }];
        if points.len() > 2 {
            let second = points[1].bearing(points[2]);
            maneuvers.push(Maneuver {
                at: points[0].distance(points[1]),
                text: format!(
                    "{} onto {}, bearing {second:.0} degrees",
                    turn_words(first, second),
                    b.name
                ),
                bearing: second,
            });
        }
        let mut route = Self::new(name, destination, points, maneuvers)?;
        route.maneuvers.push(Maneuver {
            at: route.length(),
            text: "Reach the end of the walking route".into(),
            bearing: 0.,
        });
        Ok(route)
    }
}
pub struct Guidance {
    pub route: Route,
    pub progress: f64,
    off_route: bool,
    arrived: bool,
    announced: HashSet<(usize, u8)>,
}
impl Guidance {
    pub fn new(route: Route) -> Self {
        Self {
            route,
            progress: 0.,
            off_route: false,
            arrived: false,
            announced: HashSet::new(),
        }
    }
    pub fn next(&self) -> String {
        if self.arrived {
            return self.arrival();
        }
        if let Some(m) = self
            .route
            .maneuvers
            .iter()
            .find(|m| (m.at - self.progress).abs() <= 5.)
        {
            return format!(
                "Now: {}. {:.0} metres remaining.",
                m.text,
                (self.route.length() - self.progress).max(0.)
            );
        }
        let m = self
            .route
            .maneuvers
            .iter()
            .find(|m| m.at > self.progress + 3.);
        match m {
            Some(m) => format!(
                "In {:.0} metres: {}. {:.0} metres remaining.",
                (m.at - self.progress).max(0.),
                m.text,
                (self.route.length() - self.progress).max(0.)
            ),
            None => format!(
                "Continue {:.0} metres to the route endpoint.",
                (self.route.length() - self.progress).max(0.)
            ),
        }
    }
    fn arrival(&self) -> String {
        let gap = self
            .route
            .points
            .last()
            .unwrap()
            .distance(self.route.destination);
        if gap > 15. {
            format!("Walking route complete. {} is {:.0} metres from this mapped access point; the final approach is not routed.",self.route.name,gap)
        } else {
            format!("Arrived at {}.", self.route.name)
        }
    }
    pub fn update(&mut self, p: Point) -> Option<String> {
        let (at, d) = self.route.locate(p, self.progress);
        if d > 30. {
            if !self.off_route {
                self.off_route = true;
                return Some(format!("Off route by {d:.0} metres. Shift+G recalculates from here; F returns to the route."));
            }
            return None;
        }
        let rejoined = self.off_route;
        self.off_route = false;
        self.progress = at;
        if self.route.length() - at <= 3. && p.distance(*self.route.points.last().unwrap()) <= 10. {
            if !self.arrived {
                self.arrived = true;
                return Some(self.arrival());
            }
            return None;
        }
        self.arrived = false;
        if rejoined {
            return Some(format!("Back on route. {}", self.next()));
        }
        for (i, m) in self.route.maneuvers.iter().enumerate().skip(1) {
            let remaining = m.at - at;
            if (-3.0..=35.).contains(&remaining) {
                let phase = if remaining <= 5. { 0 } else { 1 };
                if self.announced.insert((i, phase)) {
                    return Some(if phase == 0 {
                        format!("Now: {}.", m.text)
                    } else {
                        format!("In {remaining:.0} metres: {}.", m.text)
                    });
                }
            }
        }
        None
    }
    pub fn advance(&self, p: Point, step: f64) -> (Point, f64) {
        let (at, d) = self.route.locate(p, self.progress);
        if d > 3. {
            return self.route.position(at);
        }
        let stop = self
            .route
            .maneuvers
            .iter()
            .find(|m| m.at > at + 0.5)
            .map(|m| m.at)
            .unwrap_or(self.route.length());
        let target = (at + step).min(stop);
        let (point, mut heading) = self.route.position(target);
        if (target - stop).abs() < 0.01 && target < self.route.length() {
            heading = self.route.position((target - 0.1).max(0.)).1;
        }
        (point, heading)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn driving_maneuvers_use_road_language() {
        assert_eq!(
            driving_instruction("continue", "straight", "Main Street", None),
            "Continue straight to stay on Main Street"
        );
        assert_eq!(
            driving_instruction("turn", "left", "Pine Street", None),
            "Turn left onto Pine Street"
        );
        assert_eq!(
            driving_instruction("roundabout", "right", "Broadway", Some(2)),
            "At the roundabout, take exit 2 onto Broadway"
        );
        assert_eq!(
            driving_instruction("arrive", "straight", "", None),
            "Arrive at your destination"
        );
    }
    fn route() -> Route {
        let p = Point {
            lat: 47.,
            lon: -122.,
        };
        let q = p.walk(0., 100.);
        let e = q.walk(90., 100.);
        Route::new(
            "Test".into(),
            e,
            vec![p, q, e],
            vec![
                Maneuver {
                    at: 0.,
                    text: "Head North".into(),
                    bearing: 0.,
                },
                Maneuver {
                    at: 100.,
                    text: "Turn right 90 degrees".into(),
                    bearing: 90.,
                },
                Maneuver {
                    at: 200.,
                    text: "Arrive".into(),
                    bearing: 90.,
                },
            ],
        )
        .unwrap()
    }
    #[test]
    fn cached_route_validation_keeps_speed_estimates() {
        let mut cached = route();
        cached.cruise_speeds = vec![(0., 35.), (100., 55.)];
        let restored = cached.validate().unwrap();
        assert_eq!(restored.cruise_speed(150.), Some(55.));

        let mut invalid = route();
        invalid.cruise_speeds = vec![(100., f64::NAN)];
        assert!(invalid.validate().is_err());
    }
    #[test]
    fn degree_turns_wrap() {
        assert_eq!(turn_words(350., 10.), "Turn right 20 degrees");
        assert_eq!(turn_words(10., 350.), "Turn left 20 degrees");
    }
    #[test]
    fn guidance_is_quiet_and_stops_at_turns() {
        let r = route();
        let p = r.points[0];
        let mut g = Guidance::new(r);
        assert!(g.update(p.walk(0., 10.)).is_none());
        let approach = p.walk(0., 70.);
        assert!(g.update(approach).unwrap().contains("30 metres"));
        assert!(g.update(approach.walk(0., 1.)).is_none());
        let (turn, heading) = g.advance(approach, 100.);
        assert!(
            heading.abs() < 0.01,
            "Stop facing the incoming direction so a manual turn is meaningful"
        );
        assert!(turn.distance(g.route.points[1]) < 0.01);
        assert!(g.update(turn).unwrap().contains("Now"));
        assert!(g.next().contains("Turn right"));
        let (end, _) = g.advance(turn, 100.);
        assert!(g.update(end).unwrap().contains("Arrived"));
        assert!(g.update(end).is_none());
    }
    #[test]
    fn off_route_and_rejoin() {
        let r = route();
        let p = r.points[0];
        let mut g = Guidance::new(r);
        assert!(g.update(p.walk(270., 100.)).unwrap().contains("Off route"));
        assert!(g.update(p.walk(270., 110.)).is_none());
        assert!(g.update(p).unwrap().contains("Back on route"));
    }
    #[test]
    fn quiet_between_features() {
        let a = Area::demo();
        let p = a.center.walk(0., 200.);
        let mut n = Announcer::default();
        n.reset(&a, p);
        assert!(n.update(&a, p.walk(0., 1.), 0.).is_empty());
        let empty = Area {
            roads: vec![],
            places: vec![],
            context_nodes: Default::default(),
            ..a
        };
        assert!(n.update(&empty, p, 0.).is_empty());
        assert!(n.update(&empty, p.walk(0., 10.), 0.).is_empty());
        let later = Instant::now() + Duration::from_secs(21);
        assert!(n.update_at(&empty, p.walk(0., 110.), 0., later).is_empty());
        assert!(n
            .update_at(&empty, p.walk(0., 260.), 0., later)
            .join(" ")
            .contains("Continuing"));
    }
    #[test]
    fn crossing_precedes_nearer_pois() {
        let p = Point {
            lat: 40.814,
            lon: -73.944,
        };
        let json = r#"{"elements":[
            {"tags":{"amenity":"cafe","name":"Cafe"},"lat":40.81401,"lon":-73.944},
            {"tags":{"shop":"books","name":"Books"},"lat":40.81402,"lon":-73.944},
            {"tags":{"highway":"crossing"},"lat":40.8141,"lon":-73.944}
        ]}"#;
        let area = Area::parse("fixture".into(), p, 800., json).unwrap();
        let mut announcer = Announcer::default();
        announcer.reset(&area, p.walk(180., 200.));
        let events = announcer.update(&area, p, 0.);
        assert!(events.first().unwrap().starts_with("crossing"));
    }
    #[test]
    fn automatic_updates_never_read_numbered_addresses() {
        let mut area = Area::demo();
        area.places.clear();
        area.places.push(crate::map::Place {
            name: "220 West 133rd Street".into(),
            kind: "address".into(),
            street: "West 133rd Street".into(),
            point: area.center,
            group: String::new(),
            wikipedia: String::new(),
            wikidata: String::new(),
            description: String::new(),
        });
        let mut announcer = Announcer::default();
        let start = area.center.walk(180., 300.);
        announcer.reset(&area, start);
        let spoken = announcer
            .update_at(
                &area,
                area.center,
                0.,
                Instant::now() + Duration::from_secs(30),
            )
            .join(" ");
        assert!(!spoken.contains("220"));
        assert!(!spoken.contains("address"));
    }
}
