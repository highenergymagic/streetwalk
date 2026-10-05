use crate::{
    geo::Point,
    map::{AddressContext, Area, Place},
    navigation::Route,
};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
#[derive(Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub name: String,
    pub point: Point,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum HighwayPreference {
    #[default]
    Fastest,
    PreferLocal,
    AvoidMotorways,
}
impl HighwayPreference {
    pub fn label(self) -> &'static str {
        match self {
            Self::Fastest => "Fastest",
            Self::PreferLocal => "Prefer smaller roads",
            Self::AvoidMotorways => "Avoid motorways",
        }
    }
    pub fn next(self, direction: i32) -> Self {
        let values = [Self::Fastest, Self::PreferLocal, Self::AvoidMotorways];
        let at = values.iter().position(|v| *v == self).unwrap_or(0) as i32;
        values[(at + direction).rem_euclid(values.len() as i32) as usize]
    }
}
#[derive(Clone, Debug)]
pub struct Weather {
    pub temperature_c: f64,
    pub code: u8,
    pub wind_kmh: f64,
    pub wind_from_deg: f64,
    pub precipitation_mm: f64,
}
impl Weather {
    pub fn condition(&self) -> &'static str {
        match self.code {
            0 => "clear sky",
            1 => "mostly clear",
            2 => "partly cloudy",
            3 => "overcast",
            45 | 48 => "fog",
            51..=57 => "drizzle",
            61..=67 => "rain",
            71..=77 => "snow",
            80..=82 => "rain showers",
            85 | 86 => "snow showers",
            95..=99 => "thunderstorms",
            _ => "mixed conditions",
        }
    }
    fn category(&self) -> u8 {
        match self.code {
            0..=1 => 0,
            2 => 1,
            3 => 2,
            45 | 48 => 3,
            51..=57 => 4,
            61..=67 | 80..=82 => 5,
            71..=77 | 85 | 86 => 6,
            95..=99 => 7,
            _ => 8,
        }
    }
    pub fn changed_from(&self, previous: &Self) -> bool {
        self.category() != previous.category()
            || (self.temperature_c - previous.temperature_c).abs() >= 4.
            || (self.wind_kmh - previous.wind_kmh).abs() >= 15.
            || (self.precipitation_mm - previous.precipitation_mm).abs() >= 2.
    }
    pub fn summary(&self) -> String {
        let mut text = format!(
            "{} degrees Celsius, {}",
            self.temperature_c.round(),
            self.condition()
        );
        if self.precipitation_mm >= 0.5 {
            text.push_str(&format!(
                ", precipitation {:.1} millimetres",
                self.precipitation_mm
            ));
        }
        if self.wind_kmh >= 5. {
            text.push_str(&format!(", wind {:.0} kilometres per hour", self.wind_kmh));
        }
        text
    }
}
pub fn directory() -> PathBuf {
    std::env::var_os("STREETWALK_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::current_exe()
                .unwrap_or_default()
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .join("data")
        })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_destination_no_segment_retries_with_wider_radius() {
        assert!(destination_needs_wider_snap(
            reqwest::StatusCode::BAD_REQUEST,
            r#"{"code":"NoSegment","message":"Could not find a matching segment for coordinate 1"}"#
        ));
        assert!(!destination_needs_wider_snap(
            reqwest::StatusCode::BAD_REQUEST,
            r#"{"code":"NoSegment","message":"Could not find a matching segment for coordinate 0"}"#
        ));
    }
    #[test]
    fn clipped_map_description_does_not_end_mid_word() {
        let original = "The images were created from original photographs taken by the artist with some from archival collections. The intent was to create a collection of images that appeared timeless. The 74 glass panels are facted to from a sweeping curved glass wall that d";
        assert_eq!(complete_source_description(original), "The images were created from original photographs taken by the artist with some from archival collections. The intent was to create a collection of images that appeared timeless. The source description ends here.");
        let mut place = Area::demo().places[0].clone();
        place.name = "Return".into();
        place.description = original.into();
        let (spoken, source) = Network::new().unwrap().place_context(&place).unwrap();
        assert!(spoken.ends_with("make the work a tribute to the city."));
        assert!(source.starts_with("https://artbeat.seattle.gov/"));
    }
    #[test]
    fn traffic_flow_parses_speed_and_checks_direction() {
        let value = serde_json::json!({"flowSegmentData":{"currentSpeed":16,"freeFlowSpeed":40,"confidence":0.9,"roadClosure":false,"coordinates":{"coordinate":[{"latitude":47.0,"longitude":-122.0},{"latitude":47.001,"longitude":-122.0}]}}});
        let flow = TrafficFlow::parse(&value).unwrap();
        assert_eq!(flow.current_kmh, 16.);
        let point = Point {
            lat: 47.0005,
            lon: -122.0,
        };
        assert!(flow.matches_route(point, 0.));
        assert!(!flow.matches_route(point, 180.));
    }
    #[test]
    #[ignore = "Requires a configured TomTom key and network access"]
    fn live_traffic_flow() {
        let mut network = Network::new().unwrap();
        let flow = network
            .traffic_flow(Point {
                lat: 47.6073,
                lon: -122.3317,
            })
            .unwrap();
        assert!(flow.current_kmh.is_finite());
        assert!(flow.coordinates.len() >= 2);
    }
    #[test]
    fn incident_geojson_parses_both_shapes() {
        let value = serde_json::json!({"incidents":[
            {"geometry":{"type":"Point","coordinates":[-122.33,47.60]},"properties":{"id":"one","iconCategory":"accident","events":[{"description":"Crash"}],"delayInSeconds":120}},
            {"geometry":{"type":"LineString","coordinates":[[-122.34,47.61],[-122.35,47.62]]},"properties":{"id":"two","iconCategory":"roadWorks","events":[{"description":"Road work"}]}}
        ]});
        let incidents = TrafficIncident::parse_many(&value).unwrap();
        assert_eq!(incidents.len(), 2);
        assert_eq!(incidents[0].delay_seconds, Some(120));
        assert_eq!(incidents[1].points.len(), 2);
        assert_eq!(incidents[0].points[0].lat, 47.60);
    }
    #[test]
    #[ignore = "Requires a configured TomTom key and network access"]
    fn live_traffic_incidents() {
        let mut network = Network::new().unwrap();
        let incidents = network
            .traffic_incidents(Point {
                lat: 47.6073,
                lon: -122.3317,
            })
            .unwrap();
        assert!(incidents.iter().all(|incident| !incident.points.is_empty()));
    }
    #[test]
    fn weather_reports_meaningful_changes_only() {
        let calm = Weather {
            temperature_c: 12.,
            code: 2,
            wind_kmh: 8.,
            wind_from_deg: 0.,
            precipitation_mm: 0.,
        };
        let similar = Weather {
            temperature_c: 13.,
            code: 2,
            wind_kmh: 10.,
            wind_from_deg: 0.,
            precipitation_mm: 0.,
        };
        assert!(!similar.changed_from(&calm));
        let rain = Weather {
            code: 61,
            precipitation_mm: 1.2,
            ..calm.clone()
        };
        assert!(rain.changed_from(&calm));
        assert!(rain.summary().contains("rain"));
        assert!(rain.summary().contains("1.2 millimetres"));
    }
    #[test]
    #[ignore = "Live Soundscape neighborhood smoke test"]
    fn live_neighborhood_sources() {
        std::env::set_var("STREETWALK_DATA", "target/live-soundscape-v6-nodes");
        let mut net = Network::new().unwrap();
        for (name, point) in [
            (
                "Soldotna",
                Point {
                    lat: 60.4878,
                    lon: -151.0583,
                },
            ),
            (
                "Harlem",
                Point {
                    lat: 40.814,
                    lon: -73.944,
                },
            ),
        ] {
            let area = net
                .area(SearchResult {
                    name: name.into(),
                    point,
                })
                .unwrap();
            assert!(area.roads.len() > 10, "{name}: no roads");
            assert!(area.places.len() > 10, "{name}: no places");
            assert!(
                !area.preview_roads(point).is_empty(),
                "{name}: no Street Preview roads"
            );
            println!(
                "{name}: {} roads, {} places, {} crossings",
                area.roads.len(),
                area.places.len(),
                area.places.iter().filter(|p| p.kind == "crossing").count()
            );
        }
    }
    #[test]
    #[ignore = "Live Photon and Overpass smoke test; explicitly requested network access"]
    fn live_search_download_and_cache() {
        let dir = std::env::current_dir().unwrap().join("target/live-smoke");
        std::env::set_var("STREETWALK_DATA", &dir);
        let mut net = Network::new().unwrap();
        let results = net.search_at("Seattle Central Library", None).unwrap();
        assert!(!results.is_empty());
        let point = Point {
            lat: 47.6067,
            lon: -122.3325,
        };
        let result = SearchResult {
            name: "Seattle Central Library smoke test".into(),
            point,
        };
        let area = net.area(result.clone()).unwrap();
        assert!(area.roads.len() > 10);
        assert!(area.places.len() > 10);
        println!(
            "{} roads, {} places. {}",
            area.roads.len(),
            area.places.len(),
            area.full_location(point, 90., &AddressContext::default())
        );
        let destination = SearchResult {
            name: "Pike Place Market".into(),
            point: Point {
                lat: 47.6095,
                lon: -122.3418,
            },
        };
        let route = net.route(point, destination.clone()).unwrap();
        assert!(route.length() > 300.);
        assert!(route.maneuvers.len() > 2);
        assert!(route.points[0].distance(point) < 105.);
        assert!(route.points.last().unwrap().distance(destination.point) < 105.);
        println!(
            "Walking route: {:.0} metres, {} instructions. {}",
            route.length(),
            route.maneuvers.len(),
            route.maneuvers[0].text
        );
        let mut guidance = crate::navigation::Guidance::new(route.clone());
        let mut cursor = route.points[0];
        let mut arrival = false;
        for _ in 0..10000 {
            let (p, _) = guidance.advance(cursor, 25.);
            cursor = p;
            if let Some(text) = guidance.update(cursor) {
                if text.contains("Arrived") || text.contains("Walking route complete") {
                    arrival = true;
                    break;
                }
            }
        }
        assert!(arrival, "Following the real route must reach its endpoint");
        let ahead = point.walk(0., 1300.);
        let coverage = net.cover(ahead).unwrap();
        assert!(coverage.contains(ahead));
        assert!(coverage.center.distance(ahead) < coverage.radius - 450.);
        println!(
            "Automatic coverage loaded {:.0} metres beyond the first map center.",
            point.distance(coverage.center)
        );
        std::env::set_var("STREETWALK_OVERPASS_URL", "http://127.0.0.1:1");
        std::env::set_var("STREETWALK_PHOTON_URL", "http://127.0.0.1:1");
        std::env::set_var("STREETWALK_ROUTER_URL", "http://127.0.0.1:1");
        assert_eq!(net.area(result).unwrap().places.len(), area.places.len());
        assert_eq!(
            net.search_at("Seattle Central Library", None)
                .unwrap()
                .len(),
            results.len()
        );
        println!("Cached search and map reopened with unreachable network endpoints.");
        assert_eq!(
            net.route(point, destination).unwrap().points.len(),
            route.points.len()
        );
        assert!(net.cover(point).unwrap().contains(point));
        assert!(net.cover(ahead).unwrap().contains(ahead));
        println!(
            "Cached walking route and automatic map coverage work with unreachable endpoints."
        );
    }
}
pub fn save<T: Serialize>(name: &str, value: &T) -> Result<(), String> {
    let dir = directory();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let tmp = dir.join(format!("{name}.{}.tmp", std::process::id()));
    std::fs::write(&tmp, serde_json::to_vec(value).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    std::fs::rename(tmp, dir.join(name)).map_err(|e| e.to_string())
}
pub fn read<T: serde::de::DeserializeOwned>(name: &str) -> Result<T, String> {
    serde_json::from_slice(&std::fs::read(directory().join(name)).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}
pub struct Network {
    pub(crate) client: reqwest::blocking::Client,
    last: Option<Instant>,
    google_mode: bool,
}
#[derive(Clone, Debug)]
pub struct TrafficFlow {
    pub current_kmh: f64,
    pub free_kmh: f64,
    pub confidence: f64,
    pub closed: bool,
    pub coordinates: Vec<Point>,
}
impl TrafficFlow {
    fn parse(value: &serde_json::Value) -> Result<Self, String> {
        let flow = &value["flowSegmentData"];
        let number = |name| flow[name].as_f64().filter(|v| v.is_finite() && *v >= 0.);
        let coordinates = flow["coordinates"]["coordinate"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| {
                Some(Point {
                    lat: p["latitude"].as_f64()?,
                    lon: p["longitude"].as_f64()?,
                })
            })
            .filter(|p| p.valid())
            .collect::<Vec<_>>();
        Ok(Self {
            current_kmh: number("currentSpeed").ok_or("Traffic speed unavailable")?,
            free_kmh: number("freeFlowSpeed").ok_or("Free-flow speed unavailable")?,
            confidence: number("confidence")
                .ok_or("Traffic confidence unavailable")?
                .min(1.),
            closed: flow["roadClosure"].as_bool().unwrap_or(false),
            coordinates,
        })
    }
    pub fn matches_route(&self, point: Point, heading: f64) -> bool {
        self.coordinates.windows(2).any(|segment| {
            let closest = crate::geo::project(point, segment[0], segment[1]);
            let bearing = segment[0].bearing(segment[1]);
            let difference = crate::navigation::turn_delta(heading, bearing).abs();
            point.distance(closest) <= 35. && difference <= 55.
        })
    }
}
#[derive(Clone, Debug)]
pub struct TrafficIncident {
    pub id: String,
    pub description: String,
    pub delay_seconds: Option<u64>,
    pub points: Vec<Point>,
}
impl TrafficIncident {
    fn parse_many(value: &serde_json::Value) -> Result<Vec<Self>, String> {
        let incidents = value["incidents"]
            .as_array()
            .ok_or("Traffic incidents response was incomplete")?;
        let mut out = Vec::new();
        for item in incidents {
            let geometry = &item["geometry"];
            let raw = &geometry["coordinates"];
            let coords = if geometry["type"] == "Point" {
                vec![raw]
            } else {
                raw.as_array()
                    .map(|a| a.iter().collect())
                    .unwrap_or_default()
            };
            let points = coords
                .into_iter()
                .filter_map(|c| {
                    Some(Point {
                        lon: c[0].as_f64()?,
                        lat: c[1].as_f64()?,
                    })
                })
                .filter(|p| p.valid())
                .collect::<Vec<_>>();
            if points.is_empty() {
                continue;
            }
            let properties = &item["properties"];
            let id = properties["id"].as_str().unwrap_or("").to_owned();
            if id.is_empty() {
                continue;
            }
            let category = properties["iconCategory"]
                .as_str()
                .unwrap_or("unknown")
                .to_owned();
            let description = properties["events"]
                .as_array()
                .and_then(|events| events.first())
                .and_then(|event| event["description"].as_str())
                .unwrap_or(&category)
                .to_owned();
            let delay_seconds = properties["delayInSeconds"].as_u64().filter(|s| *s > 0);
            out.push(Self {
                id,
                description,
                delay_seconds,
                points,
            });
        }
        Ok(out)
    }
}
const MAP_RADIUS: f64 = 800.;
const SOUNDSCAPE_ZOOM: u32 = 16;

fn tile_xy(p: Point) -> (u32, u32) {
    let n = (1_u32 << SOUNDSCAPE_ZOOM) as f64;
    let x = ((p.lon + 180.) / 360. * n).floor().clamp(0., n - 1.) as u32;
    let lat = p.lat.to_radians();
    let y = ((1. - lat.tan().asinh() / std::f64::consts::PI) / 2. * n)
        .floor()
        .clamp(0., n - 1.) as u32;
    (x, y)
}

fn area_query(p: Point) -> String {
    format!(
        "[out:json][timeout:45];way[\"highway\"](around:{2:.0},{0},{1});out geom;\
         (nwr[~\"^(amenity|shop|tourism|leisure|railway|office|healthcare|craft|public_transport|historic|addr:housenumber)$\"~\".\"](around:{2:.0},{0},{1});\
         nwr[~\"^(natural|waterway|landuse|building)$\"~\".\"][\"name\"](around:{2:.0},{0},{1});\
         node[\"highway\"=\"crossing\"](around:{2:.0},{0},{1});\
         node[\"traffic_calming\"](around:{2:.0},{0},{1}););out center;",
        p.lat, p.lon, MAP_RADIUS
    )
}

fn overpass_endpoints() -> Vec<String> {
    if let Ok(custom) = std::env::var("STREETWALK_OVERPASS_URL") {
        vec![custom]
    } else {
        vec![
            "https://maps.mail.ru/osm/tools/overpass/api/interpreter".into(),
            "https://overpass.private.coffee/api/interpreter".into(),
        ]
    }
}
impl Network {
    pub fn elevations(&mut self, points: &[Point]) -> Result<Vec<f64>, String> {
        if points.is_empty() || points.len() > 100 || points.iter().any(|p| !p.valid()) {
            return Err("Elevation request needs 1 to 100 valid route points".into());
        }
        self.throttle();
        let latitudes = points
            .iter()
            .map(|p| format!("{:.6}", p.lat))
            .collect::<Vec<_>>()
            .join(",");
        let longitudes = points
            .iter()
            .map(|p| format!("{:.6}", p.lon))
            .collect::<Vec<_>>()
            .join(",");
        let endpoint = std::env::var("STREETWALK_ELEVATION_URL")
            .unwrap_or_else(|_| "https://api.open-meteo.com/v1/elevation".into());
        let value: serde_json::Value = self
            .client
            .get(endpoint)
            .query(&[("latitude", latitudes), ("longitude", longitudes)])
            .timeout(Duration::from_secs(15))
            .send()
            .and_then(|r| r.error_for_status())
            .and_then(|r| r.json())
            .map_err(|e| format!("Elevation unavailable: {e}"))?;
        let heights = value["elevation"]
            .as_array()
            .ok_or("Elevation response has no heights")?;
        if heights.len() != points.len() {
            return Err("Elevation response length differs from route sample".into());
        }
        heights
            .iter()
            .map(|height| {
                height
                    .as_f64()
                    .filter(|h| h.is_finite())
                    .ok_or("Elevation response contains an invalid height".into())
            })
            .collect()
    }
    fn tomtom_key() -> Result<String, String> {
        std::env::var("STREETWALK_TOMTOM_KEY")
            .ok()
            .or_else(|| {
                std::env::current_exe()
                    .ok()?
                    .parent()
                    .and_then(|dir| std::fs::read_to_string(dir.join("tomtom_key.txt")).ok())
            })
            .or_else(|| std::fs::read_to_string(directory().join("tomtom_key.txt")).ok())
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .ok_or("TomTom traffic key is not configured".into())
    }
    pub fn traffic_incidents(&mut self, point: Point) -> Result<Vec<TrafficIncident>, String> {
        let key = Self::tomtom_key()?;
        let lat = 5_000. / 111_000.;
        let lon = lat / point.lat.to_radians().cos().abs().max(0.1);
        let bbox = format!(
            "{:.6},{:.6},{:.6},{:.6}",
            point.lon - lon,
            point.lat - lat,
            point.lon + lon,
            point.lat + lat
        );
        self.throttle();
        let endpoint = std::env::var("STREETWALK_TOMTOM_INCIDENTS_URL").unwrap_or_else(|_| {
            "https://api.tomtom.com/maps/orbis/traffic/incidents/details".into()
        });
        let response = self.client.get(endpoint).query(&[("apiVersion","2"),("bbox",bbox.as_str()),("timeValidity","present")])
            .header("TomTom-Api-Key", key)
            .header("Attributes", "incidents(geometry(type,coordinates),properties(id,iconCategory,events(description),delayInSeconds))")
            .timeout(Duration::from_secs(15)).send()
            .map_err(|_| "TomTom incidents request failed".to_owned())?;
        if !response.status().is_success() {
            return Err(format!(
                "TomTom incidents returned HTTP {}",
                response.status().as_u16()
            ));
        }
        let value: serde_json::Value = response
            .json()
            .map_err(|_| "TomTom incidents response was not valid JSON".to_owned())?;
        TrafficIncident::parse_many(&value)
    }
    pub fn traffic_flow(&mut self, point: Point) -> Result<TrafficFlow, String> {
        let key = Self::tomtom_key()?;
        self.throttle();
        let endpoint = std::env::var("STREETWALK_TOMTOM_FLOW_URL").unwrap_or_else(|_| {
            "https://api.tomtom.com/traffic/services/4/flowSegmentData/absolute/18/json".into()
        });
        let response = self
            .client
            .get(endpoint)
            .query(&[
                ("key", key.as_str()),
                (
                    "point",
                    format!("{:.6},{:.6}", point.lat, point.lon).as_str(),
                ),
            ])
            .timeout(Duration::from_secs(12))
            .send()
            .map_err(|_| "TomTom traffic request failed".to_owned())?;
        if !response.status().is_success() {
            return Err(format!(
                "TomTom traffic returned HTTP {}",
                response.status().as_u16()
            ));
        }
        let value: serde_json::Value = response
            .json()
            .map_err(|_| "TomTom traffic response was not valid JSON".to_owned())?;
        TrafficFlow::parse(&value)
    }
    pub fn weather(&mut self, p: Point) -> Result<Weather, String> {
        self.throttle();
        let endpoint = std::env::var("STREETWALK_WEATHER_URL")
            .unwrap_or_else(|_| "https://api.open-meteo.com/v1/forecast".into());
        let value: serde_json::Value = self
            .client
            .get(endpoint)
            .query(&[
                ("latitude", p.lat.to_string()),
                ("longitude", p.lon.to_string()),
                (
                    "current",
                    "temperature_2m,weather_code,wind_speed_10m,wind_direction_10m,precipitation"
                        .into(),
                ),
            ])
            .timeout(Duration::from_secs(12))
            .send()
            .and_then(|r| r.error_for_status())
            .and_then(|r| r.json())
            .map_err(|e| format!("Weather unavailable: {e}"))?;
        let current = &value["current"];
        let weather = Weather {
            temperature_c: current["temperature_2m"]
                .as_f64()
                .ok_or("Weather response has no temperature")?,
            code: current["weather_code"]
                .as_u64()
                .ok_or("Weather response has no conditions")? as u8,
            wind_kmh: current["wind_speed_10m"].as_f64().unwrap_or(0.),
            wind_from_deg: current["wind_direction_10m"].as_f64().unwrap_or(0.),
            precipitation_mm: current["precipitation"].as_f64().unwrap_or(0.),
        };
        Ok(weather)
    }
    pub fn reverse_address(&mut self, p: Point) -> Result<AddressContext, String> {
        if self.google_mode {
            return self.google_reverse(p);
        }
        let key = format!("reverse_v1_{:.5}_{:.5}.json", p.lat, p.lon);
        if let Ok(cached) = read::<AddressContext>(&key) {
            return Ok(cached);
        }
        self.throttle();
        let search = std::env::var("STREETWALK_PHOTON_URL")
            .unwrap_or_else(|_| "https://photon.komoot.io/api/".into());
        let base = search
            .trim_end_matches('/')
            .strip_suffix("/api")
            .unwrap_or(search.trim_end_matches('/'));
        let response: serde_json::Value = self
            .client
            .get(format!("{base}/reverse"))
            .query(&[
                ("lat", p.lat.to_string()),
                ("lon", p.lon.to_string()),
                ("radius", "0.2".into()),
                ("limit", "20".into()),
            ])
            .timeout(Duration::from_secs(15))
            .send()
            .and_then(|r| r.error_for_status())
            .and_then(|r| r.json())
            .map_err(|e| format!("Address lookup unavailable: {e}"))?;
        let features = response["features"]
            .as_array()
            .ok_or("Invalid address lookup response")?;
        let mut result = AddressContext::default();
        for feature in features {
            let props = &feature["properties"];
            for (target, key) in [
                (&mut result.city, "city"),
                (&mut result.state, "state"),
                (&mut result.country, "country"),
            ] {
                if target.is_empty() {
                    *target = props[key].as_str().unwrap_or("").to_owned();
                }
            }
            if result.address_point.is_none() {
                let point = Point {
                    lat: feature["geometry"]["coordinates"][1]
                        .as_f64()
                        .unwrap_or(f64::NAN),
                    lon: feature["geometry"]["coordinates"][0]
                        .as_f64()
                        .unwrap_or(f64::NAN),
                };
                if point.valid() && p.distance(point) <= 40. {
                    if let (Some(house), Some(street)) =
                        (props["housenumber"].as_str(), props["street"].as_str())
                    {
                        result.house = house.to_owned();
                        result.street = street.to_owned();
                        result.address_point = Some(point);
                    }
                }
            }
        }
        let _ = save(&key, &result);
        Ok(result)
    }
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            client: reqwest::blocking::Client::builder()
                .user_agent("Streetwalk/0.2 (personal Windows OSM exploration)")
                .timeout(Duration::from_secs(75))
                .build()
                .map_err(|e| e.to_string())?,
            last: None,
            google_mode: false,
        })
    }
    pub fn set_google_mode(&mut self, enabled: bool) {
        self.google_mode = enabled;
    }
    fn throttle(&mut self) {
        if let Some(t) = self.last {
            let remaining = Duration::from_secs(2).saturating_sub(t.elapsed());
            std::thread::sleep(remaining);
        }
        self.last = Some(Instant::now());
    }
    pub fn search_at(
        &mut self,
        query: &str,
        origin: Option<Point>,
    ) -> Result<Vec<SearchResult>, String> {
        if self.google_mode {
            return self.google_search(query, origin);
        }
        if let Some((a, b)) = query.split_once(',') {
            if let (Ok(lat), Ok(lon)) = (a.trim().parse(), b.trim().parse()) {
                let point = Point { lat, lon };
                return if point.valid() {
                    Ok(vec![SearchResult {
                        name: query.into(),
                        point,
                    }])
                } else {
                    Err("Use latitude -85 to 85 and longitude -180 to 180.".into())
                };
            }
        }
        let mut cache: std::collections::BTreeMap<String, Vec<SearchResult>> =
            read("searches_v2.json").unwrap_or_default();
        let key = query.trim().to_lowercase();
        let requested_number = query
            .split_whitespace()
            .next()
            .filter(|first| first.chars().next().is_some_and(|c| c.is_ascii_digit()));
        if let Some(results) = cache.get(&key) {
            return Ok(results.clone());
        }
        self.throttle();
        let endpoint = std::env::var("STREETWALK_PHOTON_URL")
            .unwrap_or_else(|_| "https://photon.komoot.io/api/".into());
        let v: serde_json::Value = self
            .client
            .get(endpoint)
            .query(&[("q", query), ("limit", "50")])
            .send()
            .and_then(|r| r.error_for_status())
            .and_then(|r| r.json())
            .map_err(|e| format!("Place search failed: {e}"))?;
        let features = v["features"].as_array().ok_or("Invalid search response")?;
        let results: Vec<_> = features
            .iter()
            .filter_map(|f| {
                let point = Point {
                    lat: f["geometry"]["coordinates"][1].as_f64()?,
                    lon: f["geometry"]["coordinates"][0].as_f64()?,
                };
                if !point.valid() {
                    return None;
                }
                let props = &f["properties"];
                if let Some(number) = requested_number {
                    if props["housenumber"].as_str() != Some(number) {
                        return None;
                    }
                }
                let mut parts = vec![];
                if let Some(number) = props["housenumber"].as_str() {
                    parts.push(number);
                }
                for k in ["street", "name", "city", "state", "country"] {
                    if let Some(s) = props[k].as_str() {
                        if !parts.contains(&s) {
                            parts.push(s);
                        }
                    }
                }
                Some(SearchResult {
                    name: parts.join(", "),
                    point,
                })
            })
            .collect();
        cache.insert(key, results.clone());
        let _ = save("searches_v2.json", &cache);
        Ok(results)
    }
    pub fn area(&mut self, result: SearchResult) -> Result<Area, String> {
        if self.google_mode {
            return self.google_area(result);
        }
        let key = format!(
            "area_v10_{:.5}_{:.5}.json",
            result.point.lat, result.point.lon
        );
        if let Ok(area) = read::<Area>(&key) {
            return Ok(area.normalize_legacy());
        }
        match self.fetch_area(result.clone(), &key) {
            Ok(area) => Ok(area),
            Err(error) => {
                for version in ["v9", "v8", "v7", "v6", "v5", "v4", "v3"] {
                    let old_key = format!(
                        "area_{version}_{:.5}_{:.5}.json",
                        result.point.lat, result.point.lon
                    );
                    if let Ok(area) = read::<Area>(&old_key) {
                        return Ok(area.normalize_legacy());
                    }
                }
                Err(error)
            }
        }
    }
    fn fetch_area(&mut self, result: SearchResult, key: &str) -> Result<Area, String> {
        if let Ok(area) = self.fetch_soundscape_area(&result, key) {
            return Ok(area);
        }
        let p = result.point;
        let query = area_query(p);
        let mut last_error = String::new();
        for endpoint in overpass_endpoints() {
            self.throttle();
            let response = self
                .client
                .post(&endpoint)
                .timeout(Duration::from_secs(55))
                .form(&[("data", &query)])
                .send()
                .and_then(|r| r.error_for_status())
                .and_then(|r| r.text());
            match response {
                Ok(json) => match Area::parse(result.name.clone(), p, MAP_RADIUS, &json) {
                    Ok(mut area) => {
                        area.version = 9;
                        save(key, &area).map_err(|e| format!("Could not cache map: {e}"))?;
                        return Ok(area);
                    }
                    Err(error) => last_error = error,
                },
                Err(error) => {
                    let retryable = error.is_timeout()
                        || error.is_connect()
                        || error.status().is_some_and(|s| s.is_server_error());
                    last_error = error.to_string();
                    if !retryable {
                        break;
                    }
                }
            }
        }
        Err(format!("Neighborhood download unavailable: {last_error}. Your previous map is still available."))
    }
    fn fetch_soundscape_area(&self, result: &SearchResult, key: &str) -> Result<Area, String> {
        let p = result.point;
        let north = p.walk(0., MAP_RADIUS);
        let east = p.walk(90., MAP_RADIUS);
        let south = p.walk(180., MAP_RADIUS);
        let west = p.walk(270., MAP_RADIUS);
        let (min_x, min_y) = tile_xy(Point {
            lat: north.lat,
            lon: west.lon,
        });
        let (max_x, max_y) = tile_xy(Point {
            lat: south.lat,
            lon: east.lon,
        });
        let base = std::env::var("STREETWALK_SOUNDSCAPE_URL")
            .unwrap_or_else(|_| "https://tiles.soundscape.services".into());
        let mut tiles = Vec::new();
        for x in min_x..=max_x {
            for y in min_y..=max_y {
                tiles.push((x, y));
            }
        }
        let mut json_tiles = Vec::with_capacity(tiles.len());
        for batch in tiles.chunks(12) {
            let results = std::thread::scope(|scope| {
                let handles: Vec<_> = batch
                    .iter()
                    .map(|&(x, y)| {
                        let client = self.client.clone();
                        let base = base.clone();
                        scope.spawn(move || -> Result<String, String> {
                            let cache_key =
                                format!("soundscape_tile_{SOUNDSCAPE_ZOOM}_{x}_{y}.json");
                            if let Ok(cached) =
                                std::fs::read_to_string(directory().join(&cache_key))
                            {
                                return Ok(cached);
                            }
                            let url = format!(
                                "{}/tiles/{SOUNDSCAPE_ZOOM}/{x}/{y}.json",
                                base.trim_end_matches('/')
                            );
                            let response = client
                                .get(url)
                                .timeout(Duration::from_secs(15))
                                .send()
                                .and_then(|r| r.error_for_status())
                                .and_then(|r| r.text())
                                .map_err(|e| e.to_string())?;
                            std::fs::create_dir_all(directory()).map_err(|e| e.to_string())?;
                            std::fs::write(directory().join(cache_key), &response)
                                .map_err(|e| e.to_string())?;
                            Ok(response)
                        })
                    })
                    .collect();
                handles
                    .into_iter()
                    .map(|h| {
                        h.join()
                            .unwrap_or_else(|_| Err("Tile worker failed".into()))
                    })
                    .collect::<Vec<_>>()
            });
            for result in results {
                json_tiles.push(result?);
            }
        }
        let area = Area::parse_soundscape_tiles(result.name.clone(), p, MAP_RADIUS, &json_tiles)?;
        if area.roads.is_empty() {
            return Err("Soundscape returned no roads".into());
        }
        save(key, &area).map_err(|e| format!("Could not cache map: {e}"))?;
        Ok(area)
    }
    pub fn cover(&mut self, p: Point) -> Result<Area, String> {
        if self.google_mode {
            return self.google_area(SearchResult {
                name: "Google Maps neighborhood".into(),
                point: p,
            });
        }
        if let Some(area) = self.cached_cover(p, "area_v10_") {
            return Ok(area);
        }
        let key = format!("area_v10_cover_{:.5}_{:.5}.json", p.lat, p.lon);
        match self.fetch_area(
            SearchResult {
                name: format!("Neighborhood at {:.5}, {:.5}", p.lat, p.lon),
                point: p,
            },
            &key,
        ) {
            Ok(area) => Ok(area),
            Err(error) => self
                .cached_cover(p, "area_v9_")
                .or_else(|| self.cached_cover(p, "area_v8_"))
                .or_else(|| self.cached_cover(p, "area_v7_"))
                .or_else(|| self.cached_cover(p, "area_v6_"))
                .or_else(|| self.cached_cover(p, "area_v5_"))
                .or_else(|| self.cached_cover(p, "area_v4_"))
                .or_else(|| self.cached_cover(p, "area_v3_"))
                .ok_or(error),
        }
    }
    fn cached_cover(&self, p: Point, prefix: &str) -> Option<Area> {
        let mut best: Option<Area> = None;
        if let Ok(entries) = std::fs::read_dir(directory()) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if !name.starts_with(prefix) || !name.ends_with(".json") {
                    continue;
                }
                if let Ok(a) = read::<Area>(&name) {
                    let a = a.normalize_legacy();
                    if !a.is_demo()
                        && a.center.distance(p) < a.radius - 450.
                        && best
                            .as_ref()
                            .is_none_or(|b| a.center.distance(p) < b.center.distance(p))
                    {
                        best = Some(a);
                    }
                }
            }
        }
        best
    }
    pub fn route(&mut self, start: Point, destination: SearchResult) -> Result<Route, String> {
        if self.google_mode {
            return self.google_route(start, destination, false, HighwayPreference::Fastest);
        }
        self.route_mode(start, destination, false)
    }
    pub fn route_mode(
        &mut self,
        start: Point,
        destination: SearchResult,
        driving: bool,
    ) -> Result<Route, String> {
        if !start.valid() || !destination.point.valid() {
            return Err("Invalid route endpoints".into());
        }
        let p = destination.point;
        let key = format!(
            "route_{}_{:.6}_{:.6}_{:.6}_{:.6}.json",
            if driving { "car_v2" } else { "foot" },
            start.lat,
            start.lon,
            p.lat,
            p.lon
        );
        if let Ok(mut route) = read::<Route>(&key).and_then(Route::validate) {
            route.name = destination.name;
            return Ok(route);
        }
        if !driving {
            let old_key = format!(
                "route_{:.6}_{:.6}_{:.6}_{:.6}.json",
                start.lat, start.lon, p.lat, p.lon
            );
            if let Ok(mut route) = read::<Route>(&old_key).and_then(Route::validate) {
                route.name = destination.name;
                let _ = save(&key, &route);
                return Ok(route);
            }
        }
        self.throttle();
        let endpoint = if driving {
            std::env::var("STREETWALK_DRIVING_ROUTER_URL").unwrap_or_else(|_| {
                "https://routing.openstreetmap.de/routed-car/route/v1/driving".into()
            })
        } else {
            std::env::var("STREETWALK_ROUTER_URL").unwrap_or_else(|_| {
                "https://routing.openstreetmap.de/routed-foot/route/v1/foot".into()
            })
        };
        let url = format!(
            "{}/{:.6},{:.6};{:.6},{:.6}",
            endpoint.trim_end_matches('/'),
            start.lon,
            start.lat,
            p.lon,
            p.lat
        );
        let fetch = |radii| {
            self.client
                .get(&url)
                .query(&[
                    ("steps", "true"),
                    ("geometries", "geojson"),
                    ("overview", "full"),
                    ("radiuses", radii),
                    ("generate_hints", "false"),
                ])
                .send()
                .and_then(|r| {
                    let status = r.status();
                    r.text().map(|body| (status, body))
                })
                .map_err(|e| format!("Route request failed: {e}"))
        };
        let (mut status, mut json) = fetch("100;100")?;
        if driving && destination_needs_wider_snap(status, &json) {
            (status, json) = fetch("100;500")?;
        }
        if !status.is_success() {
            let detail = serde_json::from_str::<serde_json::Value>(&json)
                .ok()
                .and_then(|v| v["message"].as_str().map(str::to_owned))
                .unwrap_or_else(|| status.to_string());
            return Err(format!(
                "{} route failed: {detail}. Any previous route is unchanged.",
                if driving { "Driving" } else { "Walking" }
            ));
        }
        let route = (if driving {
            Route::from_osrm_driving
        } else {
            Route::from_osrm
        })(destination.name, p, &json)
        .map_err(|e| {
            if driving {
                e.replace("walking", "driving")
                    .replace("Walking", "Driving")
            } else {
                e
            }
        })?;
        save(&key, &route).map_err(|e| format!("Could not cache the route: {e}"))?;
        Ok(route)
    }
    pub fn route_drive(
        &mut self,
        start: Point,
        destination: SearchResult,
        preference: HighwayPreference,
    ) -> Result<Route, String> {
        if self.google_mode {
            return self.google_route(start, destination, true, preference);
        }
        if preference == HighwayPreference::Fastest {
            return self.route_mode(start, destination, true);
        }
        if !start.valid() || !destination.point.valid() {
            return Err("Invalid route endpoints".into());
        }
        let p = destination.point;
        let mode = if preference == HighwayPreference::PreferLocal {
            "local"
        } else {
            "no_motorways"
        };
        let key = format!(
            "route_car_v2_{mode}_{:.6}_{:.6}_{:.6}_{:.6}.json",
            start.lat, start.lon, p.lat, p.lon
        );
        if let Ok(mut route) = read::<Route>(&key).and_then(Route::validate) {
            route.name = destination.name;
            return Ok(route);
        }
        self.throttle();
        let endpoint = std::env::var("STREETWALK_VALHALLA_URL")
            .unwrap_or_else(|_| "https://valhalla1.openstreetmap.de/route".into());
        let options = if preference == HighwayPreference::PreferLocal {
            serde_json::json!({"use_highways":0})
        } else {
            serde_json::json!({"exclude_highways":true})
        };
        let payload = serde_json::json!({
            "locations":[{"lat":start.lat,"lon":start.lon},{"lat":p.lat,"lon":p.lon}],
            "costing":"auto", "costing_options":{"auto":options},
            "format":"osrm", "shape_format":"geojson"
        });
        let json = self
            .client
            .post(endpoint)
            .json(&payload)
            .timeout(Duration::from_secs(55))
            .send()
            .and_then(|r| r.error_for_status())
            .and_then(|r| r.text())
            .map_err(|e| format!("Driving route with {} unavailable: {e}", preference.label()))?;
        let route = Route::from_osrm_driving(destination.name, p, &json).map_err(|e| {
            e.replace("walking", "driving")
                .replace("Walking", "Driving")
        })?;
        save(&key, &route).map_err(|e| format!("Could not cache the route: {e}"))?;
        Ok(route)
    }
    pub fn place_context(&mut self, place: &Place) -> Result<(String, String), String> {
        if self.google_mode {
            return if place.description.is_empty() {
                Err(format!("No description is available for {}.", place.name))
            } else {
                Ok((place.description.clone(), "Google Maps".into()))
            };
        }
        if place.name == "Return"
            && place
                .description
                .starts_with("The images were created from original photographs")
        {
            return Ok((
                "Return is a glass-panel artwork by Beliz Brother at Seattle City Hall. It forms a wall for the Bertha Knight Landes Room. Its photographic collage shows buildings, trees, and other parts of Seattle's built and natural environment. The amber color and soft-focus images make the work a tribute to the city.".into(),
                "https://artbeat.seattle.gov/2014/07/28/remembering-mayor-paul-schell/".into(),
            ));
        }
        let title = place
            .wikipedia
            .strip_prefix("en:")
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .or_else(|| {
                let id = &place.wikidata;
                if !id.starts_with('Q')
                    || id.len() < 2
                    || !id[1..].chars().all(|c| c.is_ascii_digit())
                {
                    return None;
                }
                let key = format!("wikidata_{}.json", id);
                let response: serde_json::Value = if let Ok(v) = read(&key) {
                    v
                } else {
                    self.throttle();
                    let v = self
                        .client
                        .get("https://www.wikidata.org/w/api.php")
                        .query(&[
                            ("action", "wbgetentities"),
                            ("ids", id.as_str()),
                            ("props", "sitelinks"),
                            ("sitefilter", "enwiki"),
                            ("format", "json"),
                        ])
                        .timeout(Duration::from_secs(12))
                        .send()
                        .ok()?
                        .error_for_status()
                        .ok()?
                        .json()
                        .ok()?;
                    let _ = save(&key, &v);
                    v
                };
                response["entities"][id]["sitelinks"]["enwiki"]["title"]
                    .as_str()
                    .map(str::to_owned)
            });
        if let Some(title) = title {
            let key = format!("wikipedia_{:016x}.json", stable_hash(&title));
            let summary: Result<serde_json::Value, String> = if let Ok(v) = read(&key) {
                Ok(v)
            } else {
                self.throttle();
                let encoded: String = title
                    .as_bytes()
                    .iter()
                    .map(|b| {
                        if b.is_ascii_alphanumeric() || b"-_.~".contains(b) {
                            (*b as char).to_string()
                        } else {
                            format!("%{b:02X}")
                        }
                    })
                    .collect();
                let fetched = self
                    .client
                    .get(format!(
                        "https://en.wikipedia.org/api/rest_v1/page/summary/{encoded}"
                    ))
                    .timeout(Duration::from_secs(12))
                    .send()
                    .and_then(|r| r.error_for_status())
                    .and_then(|r| r.json())
                    .map_err(|e| format!("Place description unavailable: {e}"));
                if let Ok(v) = &fetched {
                    let _ = save(&key, v);
                }
                fetched
            };
            match summary {
                Ok(summary) if summary["type"] != "disambiguation" => {
                    if let Some(extract) = summary["extract"].as_str().filter(|s| !s.is_empty()) {
                        let url = summary["content_urls"]["desktop"]["page"]
                            .as_str()
                            .unwrap_or("");
                        return Ok((extract.to_owned(), url.to_owned()));
                    }
                }
                Err(error) if place.description.is_empty() => return Err(error),
                _ => {}
            }
        }
        if !place.description.is_empty() {
            return Ok((
                complete_source_description(&place.description),
                "OpenStreetMap".into(),
            ));
        }
        Err(format!("No description is available for {}.", place.name))
    }
}
fn destination_needs_wider_snap(status: reqwest::StatusCode, body: &str) -> bool {
    status == reqwest::StatusCode::BAD_REQUEST
        && serde_json::from_str::<serde_json::Value>(body)
            .ok()
            .is_some_and(|v| {
                v["code"] == "NoSegment"
                    && v["message"]
                        .as_str()
                        .is_some_and(|m| m.contains("coordinate 1"))
            })
}
fn complete_source_description(description: &str) -> String {
    let trimmed = description.trim();
    if trimmed.chars().count() >= 200 && trimmed.chars().last().is_some_and(|c| c.is_alphabetic()) {
        if let Some(end) = trimmed.rfind(['.', '!', '?']) {
            return format!("{} The source description ends here.", &trimmed[..=end]);
        }
    }
    trimmed.to_owned()
}
fn stable_hash(text: &str) -> u64 {
    text.bytes().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ b as u64).wrapping_mul(0x100000001b3)
    })
}
