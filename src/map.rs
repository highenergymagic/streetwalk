use crate::geo::{clock, compass, project, Point};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, OnceLock};
type ContextNodes = Arc<OnceLock<Vec<(Point, Vec<String>)>>>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Road {
    pub name: String,
    pub nodes: Vec<u64>,
    pub points: Vec<Point>,
    #[serde(default)]
    pub highway: String,
    #[serde(default)]
    pub maxspeed_kmh: Option<f64>,
    #[serde(default)]
    pub surface: String,
    #[serde(default)]
    pub smoothness: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoadEvent {
    pub point: Point,
    pub kind: String,
}
fn parse_maxspeed(raw: &str) -> Option<f64> {
    let value = raw.trim().to_ascii_lowercase();
    let (number, factor) = if let Some(n) = value.strip_suffix("mph") {
        (n.trim(), 1.609_344)
    } else if let Some(n) = value.strip_suffix("km/h") {
        (n.trim(), 1.)
    } else if let Some(n) = value.strip_suffix("kph") {
        (n.trim(), 1.)
    } else {
        (value.as_str(), 1.)
    };
    number
        .parse::<f64>()
        .ok()
        .map(|n| n * factor)
        .filter(|n| (5. ..=160.).contains(n))
}
fn car_road(kind: &str) -> bool {
    matches!(
        kind,
        "motorway"
            | "motorway_link"
            | "trunk"
            | "trunk_link"
            | "primary"
            | "primary_link"
            | "secondary"
            | "secondary_link"
            | "tertiary"
            | "tertiary_link"
            | "unclassified"
            | "residential"
            | "living_street"
            | "service"
    )
}
#[derive(Clone, Debug)]
pub struct PreviewRoad {
    pub name: String,
    pub origin: Point,
    pub destination: Point,
    pub bearing: f64,
    pub distance: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Place {
    pub name: String,
    pub kind: String,
    pub street: String,
    pub point: Point,
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub wikipedia: String,
    #[serde(default)]
    pub wikidata: String,
    #[serde(default)]
    pub description: String,
}
impl Place {
    pub fn key(&self) -> String {
        format!("{}:{:.5}:{:.5}", self.name, self.point.lat, self.point.lon)
    }
    pub fn category(&self) -> &str {
        if !self.group.is_empty() {
            return &self.group;
        }
        match self.kind.as_str() {
            "address" => "Addresses",
            "cafe" | "restaurant" | "fast food" | "bar" | "pub" | "food court" => "Food and drink",
            "supermarket" | "convenience" | "clothes" | "bakery" | "mall" => "Shopping",
            "bus stop" | "station" | "platform" | "halt" => "Transit",
            "park" | "playground" | "water" | "river" | "forest" | "nature reserve" => "Outdoors",
            "hospital" | "clinic" | "doctors" | "pharmacy" | "dentist" => "Healthcare",
            _ => "Other places",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Area {
    pub name: String,
    #[serde(default)]
    pub version: u8,
    pub center: Point,
    pub radius: f64,
    pub roads: Vec<Road>,
    pub places: Vec<Place>,
    #[serde(default)]
    pub signalized_crossings: Vec<Point>,
    #[serde(default)]
    pub road_events: Vec<RoadEvent>,
    #[serde(default)]
    pub google_street: String,
    #[serde(skip)]
    pub(crate) context_nodes: ContextNodes,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct FallbackSpeeds {
    pub built_up: f64,
    pub suburban: f64,
    pub rural: f64,
    pub open_road: f64,
}
impl Default for FallbackSpeeds {
    fn default() -> Self {
        Self {
            built_up: 35.,
            suburban: 50.,
            rural: 70.,
            open_road: 95.,
        }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AddressContext {
    pub house: String,
    pub street: String,
    pub city: String,
    pub state: String,
    pub country: String,
    pub address_point: Option<Point>,
}
#[derive(Deserialize)]
struct Response {
    elements: Vec<Element>,
    remark: Option<String>,
}
#[derive(Deserialize)]
struct Element {
    #[serde(default)]
    tags: BTreeMap<String, String>,
    lat: Option<f64>,
    lon: Option<f64>,
    center: Option<Point>,
    #[serde(default)]
    nodes: Vec<u64>,
    #[serde(default)]
    geometry: Vec<Point>,
}
impl Area {
    pub fn google_empty(center: Point) -> Self {
        Self {
            name: "Google Maps neighborhood".into(),
            version: 10,
            center,
            radius: 2000.,
            roads: vec![],
            places: vec![],
            signalized_crossings: vec![],
            road_events: vec![],
            google_street: String::new(),
            context_nodes: Arc::default(),
        }
    }
    pub fn full_location(&self, p: Point, heading: f64, address: &AddressContext) -> String {
        let local_address = self
            .nearby(p)
            .into_iter()
            .find(|i| self.places[*i].kind == "address" && p.distance(self.places[*i].point) <= 40.)
            .map(|i| {
                (
                    self.places[i].name.clone(),
                    p.distance(self.places[i].point),
                )
            });
        let geocoded_address = address.address_point.and_then(|q| {
            let distance = p.distance(q);
            (distance <= 40. && !address.house.is_empty() && !address.street.is_empty())
                .then(|| (format!("{} {}", address.house, address.street), distance))
        });
        let place = local_address.or(geocoded_address);
        let mut parts = vec![];
        if let Some((street_address, distance)) = place {
            parts.push(format!(
                "{} {street_address}",
                if distance <= 20. { "At" } else { "Near" }
            ));
        } else {
            parts.push(self.location_brief(p));
        }
        for component in [&address.city, &address.state, &address.country] {
            if !component.is_empty()
                && !parts
                    .iter()
                    .any(|part| part.eq_ignore_ascii_case(component))
            {
                parts.push(component.clone());
            }
        }
        let mut spoken = format!("{}, heading {}.", parts.join(", "), compass(heading));
        for i in self
            .nearby(p)
            .into_iter()
            .filter(|i| {
                let place = &self.places[*i];
                place.kind != "address"
                    && place.kind != "crossing"
                    && p.distance(place.point) <= 150.
            })
            .take(2)
        {
            spoken.push_str(&format!(
                " Near {} at {} o'clock.",
                self.places[i].name,
                clock(p.bearing(self.places[i].point), heading)
            ));
        }
        spoken
    }
    pub fn parse_soundscape_tiles(
        name: String,
        center: Point,
        radius: f64,
        tiles: &[String],
    ) -> Result<Self, String> {
        let mut elements = Vec::new();
        for tile in tiles {
            let response: serde_json::Value =
                serde_json::from_str(tile).map_err(|e| e.to_string())?;
            let features = response["features"]
                .as_array()
                .ok_or("Invalid Soundscape tile response")?;
            for feature in features {
                let Some(feature_type) = feature["feature_type"].as_str() else {
                    continue;
                };
                let Some(feature_value) = feature["feature_value"].as_str() else {
                    continue;
                };
                if feature_value == "gd_intersection" {
                    continue;
                }
                let mut tags = feature["properties"]
                    .as_object()
                    .cloned()
                    .unwrap_or_default();
                tags.entry(feature_type.to_owned())
                    .or_insert_with(|| serde_json::Value::String(feature_value.to_owned()));
                let geometry_type = feature["geometry"]["type"].as_str().unwrap_or("");
                let coordinates = &feature["geometry"]["coordinates"];
                let mut element = serde_json::Map::new();
                element.insert("tags".into(), serde_json::Value::Object(tags));
                match geometry_type {
                    "LineString" if feature_type == "highway" => {
                        let Some(points) = coordinates.as_array() else {
                            continue;
                        };
                        let points: Vec<_> = points.iter().filter_map(Self::tile_point).collect();
                        if points.len() < 2 {
                            continue;
                        }
                        let nodes: Vec<_> = points
                            .iter()
                            .map(|p| {
                                let lat = ((p.lat + 90.) * 1_000_000.).round() as u64;
                                let lon = ((p.lon + 180.) * 1_000_000.).round() as u64;
                                (lat << 29) | lon
                            })
                            .collect();
                        element.insert("nodes".into(), serde_json::json!(nodes));
                        element.insert(
                            "geometry".into(),
                            serde_json::Value::Array(
                                points
                                    .iter()
                                    .map(|p| serde_json::json!({"lat":p.lat,"lon":p.lon}))
                                    .collect(),
                            ),
                        );
                    }
                    _ => {
                        let point = match geometry_type {
                            "Point" => Self::tile_point(coordinates),
                            "LineString" => coordinates
                                .as_array()
                                .and_then(|a| a.get(a.len() / 2))
                                .and_then(Self::tile_point),
                            "Polygon" => coordinates.get(0).and_then(Self::tile_polygon_center),
                            "MultiPolygon" => coordinates
                                .get(0)
                                .and_then(|p| p.get(0))
                                .and_then(Self::tile_polygon_center),
                            _ => None,
                        };
                        let Some(point) = point else { continue };
                        element.insert(
                            "center".into(),
                            serde_json::json!({"lat":point.lat,"lon":point.lon}),
                        );
                    }
                }
                elements.push(serde_json::Value::Object(element));
            }
        }
        let json = serde_json::json!({"elements":elements}).to_string();
        let mut area = Self::parse(name, center, radius, &json)?;
        area.version = 9;
        let mut seen = std::collections::HashSet::new();
        area.places.retain(|p| seen.insert(p.key()));
        Ok(area)
    }
    fn tile_point(value: &serde_json::Value) -> Option<Point> {
        let coords = value.as_array()?;
        let p = Point {
            lat: coords.get(1)?.as_f64()?,
            lon: coords.first()?.as_f64()?,
        };
        p.valid().then_some(p)
    }
    fn tile_polygon_center(value: &serde_json::Value) -> Option<Point> {
        let points = value.as_array()?;
        let mut min_lat = f64::INFINITY;
        let mut max_lat = f64::NEG_INFINITY;
        let mut min_lon = f64::INFINITY;
        let mut max_lon = f64::NEG_INFINITY;
        for p in points.iter().filter_map(Self::tile_point) {
            min_lat = min_lat.min(p.lat);
            max_lat = max_lat.max(p.lat);
            min_lon = min_lon.min(p.lon);
            max_lon = max_lon.max(p.lon);
        }
        (min_lat.is_finite() && min_lon.is_finite()).then_some(Point {
            lat: (min_lat + max_lat) / 2.,
            lon: (min_lon + max_lon) / 2.,
        })
    }
    pub fn normalize_legacy(mut self) -> Self {
        for place in &mut self.places {
            if place.kind == "yes" || place.kind == "address" {
                if place
                    .name
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_digit())
                {
                    place.kind = "address".into();
                    place.group = "Addresses".into();
                    place.street.clear();
                } else {
                    place.kind = "building".into();
                    place.group = "Other places".into();
                }
            }
        }
        self
    }
    pub fn parse(name: String, center: Point, radius: f64, json: &str) -> Result<Self, String> {
        let response: Response = serde_json::from_str(json).map_err(|e| e.to_string())?;
        if let Some(remark) = response.remark {
            return Err(format!("Map server returned incomplete data: {remark}"));
        }
        let mut area = Self {
            google_street: String::new(),
            context_nodes: Arc::default(),
            name,
            version: 5,
            center,
            radius,
            roads: vec![],
            places: vec![],
            signalized_crossings: vec![],
            road_events: vec![],
        };
        for e in response.elements {
            let tag = |k: &str| e.tags.get(k).cloned().unwrap_or_default();
            if e.tags.contains_key("highway")
                && e.geometry.len() > 1
                && e.geometry.iter().all(|p| p.valid())
            {
                let event_kind = tag("traffic_calming");
                if matches!(
                    event_kind.as_str(),
                    "bump" | "hump" | "cushion" | "table" | "rumble_strip"
                ) {
                    let point = e.geometry[e.geometry.len() / 2];
                    if center.distance(point) <= radius {
                        area.road_events.push(RoadEvent {
                            point,
                            kind: event_kind,
                        });
                    }
                }
                let name = if tag("name").is_empty() {
                    tag("highway").replace('_', " ")
                } else {
                    tag("name")
                };
                area.roads.push(Road {
                    name,
                    nodes: e.nodes,
                    points: e.geometry,
                    highway: tag("highway"),
                    maxspeed_kmh: parse_maxspeed(&tag("maxspeed")),
                    surface: tag("surface"),
                    smoothness: tag("smoothness"),
                });
                continue;
            }
            if let Some(point) = e.center.or_else(|| {
                Some(Point {
                    lat: e.lat?,
                    lon: e.lon?,
                })
            }) {
                let kind = tag("traffic_calming");
                if point.valid()
                    && center.distance(point) <= radius
                    && matches!(
                        kind.as_str(),
                        "bump" | "hump" | "cushion" | "table" | "rumble_strip"
                    )
                {
                    area.road_events.push(RoadEvent { point, kind });
                }
            }
            let kind = [
                "amenity",
                "shop",
                "tourism",
                "leisure",
                "railway",
                "office",
                "healthcare",
                "craft",
                "public_transport",
                "historic",
                "natural",
                "waterway",
                "landuse",
                "building",
            ]
            .iter()
            .find_map(|k| e.tags.get(*k))
            .cloned()
            .or_else(|| (tag("highway") == "crossing").then(|| "crossing".into()))
            .or_else(|| e.tags.get("addr:housenumber").map(|_| "address".into()));
            if let Some(kind) = kind {
                let point = e.center.or_else(|| {
                    Some(Point {
                        lat: e.lat?,
                        lon: e.lon?,
                    })
                });
                if let Some(point) = point.filter(|p| p.valid() && center.distance(*p) <= radius) {
                    if kind == "crossing"
                        && (tag("crossing") == "traffic_signals"
                            || tag("crossing:signals") == "yes")
                    {
                        area.signalized_crossings.push(point);
                    }
                    let kind = if !tag("addr:housenumber").is_empty()
                        && tag("name").is_empty()
                        && (kind == "address" || e.tags.get("building") == Some(&kind))
                    {
                        "address".to_string()
                    } else if e.tags.get("building") == Some(&kind) {
                        "building".to_string()
                    } else {
                        kind
                    };
                    if ["natural", "waterway", "landuse", "building"]
                        .iter()
                        .any(|key| e.tags.contains_key(*key))
                        && tag("name").is_empty()
                        && tag("addr:housenumber").is_empty()
                    {
                        continue;
                    }
                    let address = format!("{} {}", tag("addr:housenumber"), tag("addr:street"))
                        .trim()
                        .to_string();
                    let name = if !tag("name").is_empty() {
                        tag("name")
                    } else if !address.is_empty() {
                        address
                    } else {
                        kind.replace('_', " ")
                    };
                    area.places.push(Place {
                        name,
                        kind: kind.replace('_', " "),
                        street: if kind == "address" {
                            String::new()
                        } else {
                            tag("addr:street")
                        },
                        point,
                        wikipedia: tag("wikipedia"),
                        wikidata: tag("wikidata"),
                        description: tag("description"),
                        group: if kind == "crossing" {
                            "Crossings"
                        } else if kind == "address" {
                            "Addresses"
                        } else if [
                            "cafe",
                            "restaurant",
                            "fast_food",
                            "bar",
                            "pub",
                            "food_court",
                        ]
                        .contains(&kind.as_str())
                        {
                            "Food and drink"
                        } else if ["hospital", "clinic", "doctors", "pharmacy", "dentist"]
                            .contains(&kind.as_str())
                        {
                            "Healthcare"
                        } else if e.tags.contains_key("shop") {
                            "Shopping"
                        } else if e.tags.contains_key("amenity") {
                            "Community and services"
                        } else if e.tags.contains_key("tourism") || e.tags.contains_key("historic")
                        {
                            "Sights"
                        } else if e.tags.contains_key("leisure")
                            || e.tags.contains_key("natural")
                            || e.tags.contains_key("waterway")
                            || e.tags.contains_key("landuse")
                        {
                            "Outdoors"
                        } else if e.tags.contains_key("railway")
                            || e.tags.contains_key("public_transport")
                        {
                            "Transit"
                        } else if e.tags.contains_key("healthcare") {
                            "Healthcare"
                        } else if e.tags.contains_key("office") || e.tags.contains_key("craft") {
                            "Workplaces"
                        } else {
                            "Other places"
                        }
                        .into(),
                    });
                }
            }
        }
        Ok(area)
    }
    pub fn contains(&self, p: Point) -> bool {
        p.valid() && self.center.distance(p) <= self.radius - 30.
    }
    pub fn nearest_road(&self, p: Point) -> Option<(&Road, Point, f64)> {
        self.roads
            .iter()
            .flat_map(|r| {
                r.points.windows(2).map(move |s| {
                    let q = project(p, s[0], s[1]);
                    (r, q, p.distance(q))
                })
            })
            .min_by(|a, b| a.2.total_cmp(&b.2))
    }
    pub fn driving_speed(&self, p: Point, heading: f64, speeds: FallbackSpeeds) -> (f64, bool) {
        let candidate = self
            .roads
            .iter()
            .filter(|r| car_road(&r.highway))
            .flat_map(|r| {
                r.points.windows(2).map(move |s| {
                    let q = project(p, s[0], s[1]);
                    let bearing = s[0].bearing(s[1]);
                    let difference = (bearing - heading).rem_euclid(180.);
                    let aligned = difference.min(180. - difference) <= 45.;
                    (r, p.distance(q), aligned)
                })
            })
            .filter(|(_, distance, aligned)| *distance <= 25. && *aligned)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((road, _, _)) = candidate {
            if let Some(limit) = road.maxspeed_kmh {
                return (limit, true);
            }
            let speed = match road.highway.as_str() {
                "motorway" | "trunk" => speeds.open_road,
                "motorway_link" | "trunk_link" => speeds.rural,
                "living_street" | "service" => (speeds.built_up * 0.6).max(10.),
                "residential" => speeds.built_up,
                _ => {
                    let nearby = self
                        .places
                        .iter()
                        .filter(|place| {
                            place.kind != "address"
                                && place.kind != "crossing"
                                && p.distance(place.point) <= 300.
                        })
                        .take(12)
                        .count();
                    if nearby >= 12 {
                        speeds.built_up
                    } else if nearby >= 3 {
                        speeds.suburban
                    } else {
                        speeds.rural
                    }
                }
            };
            return (speed.clamp(10., 130.), false);
        }
        (speeds.suburban.clamp(10., 130.), false)
    }
    pub fn driving_texture(&self, p: Point) -> f64 {
        self.roads
            .iter()
            .filter(|road| car_road(&road.highway))
            .filter_map(|road| {
                road.points
                    .windows(2)
                    .map(|segment| p.distance(project(p, segment[0], segment[1])))
                    .min_by(f64::total_cmp)
                    .map(|distance| (road, distance))
            })
            .filter(|(_, distance)| *distance <= 25.)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map_or(1., |(road, _)| {
                let class: f64 = match road.highway.as_str() {
                    "motorway" | "trunk" => 0.75,
                    "service" | "living_street" => 1.15,
                    "track" => 1.4,
                    _ => 1.,
                };
                let surface: f64 = match road.surface.as_str() {
                    "asphalt" | "paved" => 1.,
                    "concrete" | "concrete:plates" => 1.12,
                    "paving_stones" | "sett" | "cobblestone" => 1.32,
                    "gravel" | "fine_gravel" | "compacted" => 1.38,
                    "dirt" | "ground" | "earth" | "unpaved" => 1.48,
                    _ => 1.,
                };
                (class * surface).clamp(0.5, 1.5)
            })
    }
    pub fn driving_bumpiness(&self, p: Point) -> f64 {
        // The downloaded neighborhood is much smaller than 20 miles. Treat its
        // observed POIs as a local settlement proxy, then blend road class.
        let observed = self
            .places
            .iter()
            .filter(|place| {
                place.kind != "address"
                    && place.kind != "crossing"
                    && p.distance(place.point) < 1_000.
            })
            .count() as f64;
        let sparse = (1. - observed / 45.).clamp(0., 1.).powf(1.5);
        let class = self.driving_texture(p);
        let smoothness = self
            .roads
            .iter()
            .filter(|road| car_road(&road.highway))
            .filter_map(|road| {
                road.points
                    .windows(2)
                    .map(|segment| p.distance(project(p, segment[0], segment[1])))
                    .min_by(f64::total_cmp)
                    .map(|distance| (road, distance))
            })
            .filter(|(_, distance)| *distance <= 25.)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map_or(1., |(road, _)| match road.smoothness.as_str() {
                "excellent" => 0.7,
                "good" => 0.85,
                "intermediate" => 1.,
                "bad" => 1.25,
                "very_bad" | "horrible" => 1.5,
                _ => 1.,
            });
        ((0.12 + sparse * 0.72) * class * smoothness).clamp(0.08, 1.)
    }
    pub fn driving_urbanity(&self, p: Point) -> f64 {
        let places = self
            .places
            .iter()
            .filter(|place| {
                place.kind != "address"
                    && place.kind != "crossing"
                    && p.distance(place.point) <= 600.
            })
            .take(24)
            .count() as f64;
        let road = self
            .roads
            .iter()
            .filter(|road| car_road(&road.highway))
            .filter_map(|road| {
                road.points
                    .windows(2)
                    .map(|segment| p.distance(project(p, segment[0], segment[1])))
                    .min_by(f64::total_cmp)
                    .map(|distance| (road, distance))
            })
            .filter(|(_, distance)| *distance <= 30.)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map_or(0., |(road, _)| match road.highway.as_str() {
                "residential" | "living_street" => 0.55,
                "service" | "tertiary" => 0.4,
                "motorway" | "trunk" => 0.08,
                _ => 0.25,
            });
        (places / 20.).clamp(0., 1.).max(road)
    }
    pub fn preview_roads(&self, p: Point) -> Vec<PreviewRoad> {
        let Some((node, origin, distance)) = self
            .roads
            .iter()
            .flat_map(|road| road.nodes.iter().zip(&road.points))
            .map(|(id, point)| (*id, *point, p.distance(*point)))
            .min_by(|a, b| a.2.total_cmp(&b.2))
        else {
            return vec![];
        };
        if distance > 120. {
            return vec![];
        }
        let mut roads_at_node: HashMap<u64, HashSet<&str>> = HashMap::new();
        for road in &self.roads {
            for id in &road.nodes {
                roads_at_node.entry(*id).or_default().insert(&road.name);
            }
        }
        let mut options = Vec::new();
        for road in &self.roads {
            for (index, id) in road.nodes.iter().enumerate() {
                if *id != node || road.points.len() != road.nodes.len() {
                    continue;
                }
                for direction in [-1isize, 1] {
                    let next = index as isize + direction;
                    if next < 0 || next >= road.points.len() as isize {
                        continue;
                    }
                    let mut position = index as isize;
                    let mut length = 0.;
                    loop {
                        let following = position + direction;
                        if following < 0 || following >= road.points.len() as isize {
                            break;
                        }
                        length += road.points[position as usize]
                            .distance(road.points[following as usize]);
                        position = following;
                        let end = position == 0 || position as usize == road.points.len() - 1;
                        let junction = roads_at_node
                            .get(&road.nodes[position as usize])
                            .is_some_and(|names| names.len() > 1);
                        if end || junction {
                            break;
                        }
                    }
                    let destination = road.points[position as usize];
                    if length < 1. || destination == origin {
                        continue;
                    }
                    options.push(PreviewRoad {
                        name: road.name.clone(),
                        origin,
                        destination,
                        bearing: origin.bearing(road.points[next as usize]),
                        distance: length,
                    });
                }
            }
        }
        options.sort_by(|a, b| a.bearing.total_cmp(&b.bearing));
        let mut seen = HashSet::new();
        options
            .retain(|road| seen.insert((road.name.clone(), (road.bearing / 15.).round() as i32)));
        options.truncate(16);
        options
    }
    pub fn nearby(&self, p: Point) -> Vec<usize> {
        let mut indices: Vec<_> = (0..self.places.len()).collect();
        indices.sort_by(|a, b| {
            p.distance(self.places[*a].point)
                .total_cmp(&p.distance(self.places[*b].point))
        });
        indices
    }
    pub fn exploration_places(&self, p: Point, heading: f64, ahead: bool) -> Vec<usize> {
        let mut indices = self.nearby(p);
        indices.retain(|&i| {
            let place = &self.places[i];
            let distance = p.distance(place.point);
            if distance > self.radius.min(800.)
                || place.kind == "address"
                || place.kind == "crossing"
                || place.name.eq_ignore_ascii_case(&place.kind)
            {
                return false;
            }
            let relative = (p.bearing(place.point) - heading + 180.).rem_euclid(360.) - 180.;
            !ahead || relative.abs() <= 45.
        });
        if ahead {
            indices.truncate(5);
            return indices;
        }
        let mut quadrants = [None; 4];
        for i in indices {
            let bearing = p.bearing(self.places[i].point);
            let quadrant = ((bearing + 45.).rem_euclid(360.) / 90.) as usize;
            if quadrants[quadrant].is_none() {
                quadrants[quadrant] = Some(i);
            }
            if quadrants.iter().all(Option::is_some) {
                break;
            }
        }
        quadrants.into_iter().flatten().collect()
    }
    pub fn describe_place(&self, index: usize, p: Point, heading: f64) -> String {
        let place = &self.places[index];
        let street = if !place.street.is_empty() && !place.name.contains(&place.street) {
            format!(" on {}", place.street)
        } else if place.kind != "address" && place.kind != "building" && place.street.is_empty() {
            self.nearest_road(place.point)
                .filter(|(_, _, d)| *d < 60.)
                .map(|(r, _, _)| format!(" near {}", r.name))
                .unwrap_or_default()
        } else {
            String::new()
        };
        let kind = if place.kind == "address"
            || place.kind == "building"
            || place.kind == "yes"
            || place.name.eq_ignore_ascii_case(&place.kind)
        {
            String::new()
        } else {
            format!("; {}", place.kind)
        };
        format!(
            "{}{}{}, {:.0} metres at {} o'clock",
            place.name,
            street,
            kind,
            p.distance(place.point),
            clock(p.bearing(place.point), heading)
        )
    }
    pub fn callout_place(&self, index: usize, p: Point, heading: f64) -> String {
        let place = &self.places[index];
        format!(
            "{}, {} o'clock",
            place.name,
            clock(p.bearing(place.point), heading)
        )
    }
    pub fn is_demo(&self) -> bool {
        self.name == "Demo neighborhood (fictional)"
    }
    pub fn context(&self, p: Point) -> Option<(String, String)> {
        if self.version == 10 {
            return (!self.google_street.is_empty() && self.contains(p)).then(|| {
                (
                    format!("google:street:{}", self.google_street),
                    format!("On {}", self.google_street),
                )
            });
        }
        // Only shared OSM nodes count as intersections: bridges crossing in geometry do not.
        let nodes = self.context_nodes.get_or_init(|| {
            let mut nodes: HashMap<u64, (Point, Vec<String>)> = HashMap::new();
            for road in &self.roads {
                for (id, point) in road.nodes.iter().zip(&road.points) {
                    let entry = nodes.entry(*id).or_insert((*point, vec![]));
                    if !entry.1.contains(&road.name) {
                        entry.1.push(road.name.clone());
                    }
                }
            }
            nodes
                .into_values()
                .filter(|(_, names)| names.len() > 1)
                .collect()
        });
        let corner = nodes
            .iter()
            .filter(|(q, _)| p.distance(*q) < 18.)
            .min_by(|a, b| p.distance(a.0).total_cmp(&p.distance(b.0)));
        if let Some((q, names)) = corner {
            let display = names.join(" and ");
            let mut names = names.clone();
            names.sort_unstable();
            Some((
                format!("corner:{:.5}:{:.5}:{}", q.lat, q.lon, names.join("|")),
                format!("At corner of {display}"),
            ))
        } else if let Some((road, _, d)) = self.nearest_road(p) {
            if d < 25. {
                Some((format!("street:{}", road.name), format!("On {}", road.name)))
            } else {
                None
            }
        } else {
            None
        }
    }
    pub fn location_brief(&self, p: Point) -> String {
        if self.version == 10 {
            return self
                .context(p)
                .map(|c| c.1)
                .unwrap_or_else(|| "Google Maps location".into());
        }
        self.context(p).map(|c| c.1).unwrap_or_else(|| {
            self.nearest_road(p)
                .map(|(r, _, d)| format!("{d:.0} metres from {}", r.name))
                .unwrap_or_else(|| "No mapped streets nearby".into())
        })
    }
    pub fn demo() -> Self {
        Self::parse(
            "Demo neighborhood (fictional)".into(),
            Point {
                lat: 47.6,
                lon: -122.33,
            },
            1000.,
            include_str!("../assets/demo.json"),
        )
        .unwrap()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_place_density_reduces_road_defect_proxy() {
        let mut area = Area::demo();
        area.places.clear();
        let sparse = area.driving_bumpiness(area.center);
        let quiet_traffic = area.driving_urbanity(area.center);
        let mut place = Place {
            name: "Shop".into(),
            kind: "store".into(),
            street: String::new(),
            point: area.center,
            group: String::new(),
            wikipedia: String::new(),
            wikidata: String::new(),
            description: String::new(),
        };
        for i in 0..45 {
            place.point = area.center.walk(90., i as f64 * 10.);
            area.places.push(place.clone());
        }
        assert!(area.driving_bumpiness(area.center) < sparse);
        assert!(area.driving_urbanity(area.center) > quiet_traffic);
    }
    #[test]
    fn shared_nodes_make_corner() {
        let a = Area::demo();
        assert!(a
            .full_location(a.center, 90., &AddressContext::default())
            .contains("corner of Third Avenue and Park Street"));
    }
    #[test]
    fn boundary_and_poi() {
        let a = Area::demo();
        assert!(!a.contains(a.center.walk(0., 1000.)));
        assert_eq!(a.places.len(), 3);
        assert!(a.describe_place(0, a.center, 0.).contains("metres"));
    }
    #[test]
    fn retains_place_context_links_from_map_data() {
        let center = Point {
            lat: 47.6095,
            lon: -122.3418,
        };
        let json = r#"{"elements":[{"lat":47.6095,"lon":-122.3418,"tags":{"tourism":"attraction","name":"Pike Place Market","wikipedia":"en:Pike Place Market","wikidata":"Q1373418","description":"Historic public market"}}]}"#;
        let area = Area::parse("Seattle".into(), center, 800., json).unwrap();
        assert_eq!(area.places[0].wikipedia, "en:Pike Place Market");
        assert_eq!(area.places[0].wikidata, "Q1373418");
        assert_eq!(area.places[0].description, "Historic public market");
        let tile = r#"{"features":[{"feature_type":"tourism","feature_value":"attraction","geometry":{"type":"Point","coordinates":[-122.3418,47.6095]},"properties":{"name":"Pike Place Market","wikipedia":"en:Pike Place Market","wikidata":"Q1373418","description":"Historic public market"}}]}"#;
        let from_tile =
            Area::parse_soundscape_tiles("Seattle".into(), center, 800., &[tile.into()]).unwrap();
        assert_eq!(from_tile.places[0].wikipedia, "en:Pike Place Market");
    }
    #[test]
    fn old_cached_places_still_load() {
        let place: Place = serde_json::from_str(r#"{"name":"Library","kind":"library","street":"","point":{"lat":47.0,"lon":-122.0},"group":""}"#).unwrap();
        assert!(place.wikipedia.is_empty());
        assert!(place.wikidata.is_empty());
    }
    #[test]
    fn drive_speed_uses_tagged_limit_and_road_type_fallback() {
        assert!((parse_maxspeed("25 mph").unwrap() - 40.2336).abs() < 0.01);
        assert_eq!(parse_maxspeed("50"), Some(50.));
        assert_eq!(parse_maxspeed("signals"), None);
        let center = Point {
            lat: 47.0,
            lon: -122.0,
        };
        let json = r#"{"elements":[{"tags":{"highway":"residential","name":"Main Street","maxspeed":"25 mph"},"geometry":[{"lat":46.999,"lon":-122.0},{"lat":47.001,"lon":-122.0}]}]}"#;
        let area = Area::parse("Test".into(), center, 800., json).unwrap();
        let (speed, tagged) = area.driving_speed(center, 0., FallbackSpeeds::default());
        assert!(tagged);
        assert!((speed - 40.2336).abs() < 0.01);
        let no_tag = json.replace(",\"maxspeed\":\"25 mph\"", "");
        let area = Area::parse("Test".into(), center, 800., &no_tag).unwrap();
        assert_eq!(
            area.driving_speed(center, 0., FallbackSpeeds::default()),
            (35., false)
        );
    }
    #[test]
    fn mapped_surface_and_traffic_calming_change_drive_texture() {
        let center = Point {
            lat: 47.,
            lon: -122.,
        };
        let json = r#"{"elements":[{"tags":{"highway":"residential","surface":"gravel","smoothness":"bad"},"geometry":[{"lat":46.999,"lon":-122.0},{"lat":47.001,"lon":-122.0}]},{"lat":47.0,"lon":-122.0,"tags":{"traffic_calming":"rumble_strip"}}]}"#;
        let area = Area::parse("Test".into(), center, 800., json).unwrap();
        assert!(area.driving_texture(center) > 1.3);
        assert!(area.driving_bumpiness(center) > 0.4);
        assert_eq!(area.road_events.len(), 1);
        assert_eq!(area.road_events[0].kind, "rumble_strip");
    }
    #[test]
    fn full_where_am_i_uses_address_admin_and_two_clock_directions() {
        let mut area = Area::demo();
        area.places = vec![
            Place {
                name: "123 Main Street".into(),
                kind: "address".into(),
                street: String::new(),
                point: area.center,
                group: String::new(),
                wikipedia: String::new(),
                wikidata: String::new(),
                description: String::new(),
            },
            Place {
                name: "Library".into(),
                kind: "library".into(),
                street: String::new(),
                point: area.center.walk(0., 30.),
                group: String::new(),
                wikipedia: String::new(),
                wikidata: String::new(),
                description: String::new(),
            },
            Place {
                name: "Cafe".into(),
                kind: "cafe".into(),
                street: String::new(),
                point: area.center.walk(90., 40.),
                group: String::new(),
                wikipedia: String::new(),
                wikidata: String::new(),
                description: String::new(),
            },
        ];
        let context = AddressContext {
            city: "Seattle".into(),
            state: "Washington".into(),
            country: "United States".into(),
            ..Default::default()
        };
        assert_eq!(
            area.full_location(area.center, 0., &context),
            "At 123 Main Street, Seattle, Washington, United States, heading North. Near Library at 12 o'clock. Near Cafe at 3 o'clock."
        );
    }
    #[test]
    fn spoken_clock_has_no_leading_zero() {
        let mut area = Area::demo();
        area.places.clear();
        area.places.push(Place {
            name: "Library".into(),
            kind: "library".into(),
            street: String::new(),
            point: area.center.walk(240., 40.),
            group: String::new(),
            wikipedia: String::new(),
            wikidata: String::new(),
            description: String::new(),
        });
        assert_eq!(area.callout_place(0, area.center, 0.), "Library, 8 o'clock");
        assert!(area
            .describe_place(0, area.center, 0.)
            .ends_with("8 o'clock"));
    }
    #[test]
    fn exploration_actions_choose_directional_named_places() {
        let mut area = Area::demo();
        area.places.clear();
        for (name, bearing) in [
            ("North cafe", 0.),
            ("East shop", 90.),
            ("South park", 180.),
            ("West museum", 270.),
        ] {
            area.places.push(Place {
                name: name.into(),
                kind: "cafe".into(),
                street: String::new(),
                point: area.center.walk(bearing, 100.),
                group: String::new(),
                wikipedia: String::new(),
                wikidata: String::new(),
                description: String::new(),
            });
        }
        area.places.push(Place {
            name: "12 Park Street".into(),
            kind: "address".into(),
            street: String::new(),
            point: area.center.walk(0., 10.),
            group: String::new(),
            wikipedia: String::new(),
            wikidata: String::new(),
            description: String::new(),
        });
        assert_eq!(area.exploration_places(area.center, 0., false).len(), 4);
        let ahead = area.exploration_places(area.center, 90., true);
        assert_eq!(ahead.len(), 1);
        assert_eq!(area.places[ahead[0]].name, "East shop");
    }
    #[test]
    fn street_preview_follows_roads_to_next_decision_point() {
        let area = Area::demo();
        let options = area.preview_roads(area.center);
        assert_eq!(options.len(), 4);
        assert!(options
            .iter()
            .any(|road| road.name == "Third Avenue" && road.bearing < 45.));
        assert!(options
            .iter()
            .any(|road| road.name == "Park Street" && (road.bearing - 90.).abs() < 5.));
        assert!(options.iter().all(|road| road.distance > 100.));
    }
    #[test]
    fn rejects_partial_response() {
        assert!(Area::parse(
            "x".into(),
            Point { lat: 0., lon: 0. },
            1000.,
            r#"{"elements":[],"remark":"timeout"}"#
        )
        .is_err());
    }
    #[test]
    fn bridge_crossing_is_not_an_intersection() {
        let mut a = Area::demo();
        a.roads[1].nodes = vec![40, 20, 50];
        assert!(!a.location_brief(a.center).contains("corner"));
    }
    #[test]
    fn address_and_named_landscape_are_browsable() {
        let center = Point {
            lat: 60.4878,
            lon: -151.0583,
        };
        let json = r#"{"elements":[
            {"tags":{"building":"yes","addr:housenumber":"123","addr:street":"Binkley Street"},"lat":60.4878,"lon":-151.0583},
            {"tags":{"natural":"water","name":"Soldotna Creek"},"lat":60.488,"lon":-151.0583},
            {"tags":{"building":"yes"},"lat":60.488,"lon":-151.059}
        ]}"#;
        let area = Area::parse("Soldotna".into(), center, 1200., json).unwrap();
        assert_eq!(area.places.len(), 2);
        assert!(area.places.iter().any(|p| p.name == "123 Binkley Street"));
        assert!(area.places.iter().any(|p| p.category() == "Addresses"));
        assert!(area.places.iter().any(|p| p.name == "Soldotna Creek"));
    }
    #[test]
    fn named_building_and_address_do_not_make_noisy_location() {
        let center = Point {
            lat: 40.814,
            lon: -73.944,
        };
        let json = r#"{"elements":[
            {"tags":{"highway":"residential","name":"West 133rd Street"},"nodes":[1,2],"geometry":[{"lat":40.813,"lon":-73.944},{"lat":40.815,"lon":-73.944}]},
            {"tags":{"building":"yes","addr:housenumber":"220","addr:street":"West 133rd Street"},"lat":40.8142,"lon":-73.944},
            {"tags":{"building":"yes","name":"Public School 119 (historical)","addr:street":"West 133rd Street"},"lat":40.81425,"lon":-73.944}
        ]}"#;
        let area = Area::parse("Harlem".into(), center, 1200., json).unwrap();
        let spoken = area.location_brief(center);
        assert!(spoken.contains("West 133rd Street"));
        assert!(!spoken.contains("; yes"));
        assert!(!spoken.contains("; address"));
        assert!(!spoken.contains("220 West 133rd Street"));
        assert!(area
            .full_location(center, 0., &AddressContext::default())
            .contains("Near Public School 119"));
        assert_eq!(
            area.places
                .iter()
                .filter(|p| p.category() == "Addresses")
                .count(),
            1
        );
    }
    #[test]
    fn crossing_node_is_a_safety_place() {
        let center = Point {
            lat: 40.814,
            lon: -73.944,
        };
        let json = r#"{"elements":[{"tags":{"highway":"crossing"},"lat":40.8141,"lon":-73.944}]}"#;
        let area = Area::parse("Crossing".into(), center, 800., json).unwrap();
        assert_eq!(area.places.len(), 1);
        assert_eq!(area.places[0].kind, "crossing");
        assert_eq!(area.places[0].category(), "Crossings");
        assert!(area.signalized_crossings.is_empty());
        let signaled = json.replace(
            "\"highway\":\"crossing\"",
            "\"highway\":\"crossing\",\"crossing\":\"traffic_signals\"",
        );
        let area = Area::parse("Crossing".into(), center, 800., &signaled).unwrap();
        assert_eq!(area.signalized_crossings.len(), 1);
    }
}
