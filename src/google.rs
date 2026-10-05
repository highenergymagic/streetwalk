use crate::{
    data::{HighwayPreference, Network, SearchResult},
    geo::Point,
    map::{AddressContext, Area, Place},
    navigation::{Maneuver, Route},
};
use serde_json::{json, Value};
use std::collections::HashSet;

const SEARCH_RADII: [f64; 4] = [1_000., 5_000., 25_000., 50_000.];
const NEARBY_TYPES: [&[&str]; 3] = [
    &[
        "restaurant",
        "cafe",
        "coffee_shop",
        "fast_food_restaurant",
        "bakery",
        "bar",
    ],
    &[
        "store",
        "supermarket",
        "convenience_store",
        "shopping_mall",
        "grocery_store",
        "pharmacy",
        "bank",
    ],
    &[
        "park",
        "tourist_attraction",
        "museum",
        "library",
        "school",
        "university",
        "bus_station",
        "train_station",
        "hospital",
    ],
];

fn response_json(
    request: reqwest::blocking::RequestBuilder,
    action: &str,
) -> Result<Value, String> {
    let response = request
        .send()
        .map_err(|e| format!("Google {action} failed: {e}"))?;
    let status = response.status();
    let value: Value = response
        .json()
        .map_err(|e| format!("Google {action} returned invalid JSON: {e}"))?;
    if !status.is_success() {
        let detail = value["error"]["message"]
            .as_str()
            .unwrap_or("Check the API key, enabled APIs, billing, and quota.");
        return Err(format!("Google {action} failed ({status}): {detail}"));
    }
    Ok(value)
}

pub fn key() -> Result<String, String> {
    std::env::var("STREETWALK_GOOGLE_KEY")
        .ok()
        .or_else(|| {
            let exe = std::env::current_exe().ok()?;
            let dir = exe.parent()?;
            std::fs::read_to_string(dir.join("google_key.txt"))
                .ok()
                .or_else(|| std::fs::read_to_string(dir.join("data/google_key.txt")).ok())
        })
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "Google Maps key missing. Put it in google_key.txt beside Streetwalk.exe, or set STREETWALK_GOOGLE_KEY.".into())
}

fn point(value: &Value) -> Option<Point> {
    let p = Point {
        lat: value["latitude"].as_f64()?,
        lon: value["longitude"].as_f64()?,
    };
    p.valid().then_some(p)
}

fn place_name(value: &Value) -> Option<String> {
    value["displayName"]["text"]
        .as_str()
        .or_else(|| value["formattedAddress"].as_str())
        .map(str::to_owned)
}

fn parse_search(value: &Value) -> Vec<SearchResult> {
    value["places"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| {
            Some(SearchResult {
                name: match (place_name(v)?, v["formattedAddress"].as_str()) {
                    (name, Some(address)) if !address.is_empty() && address != name => {
                        format!("{name}, {address}")
                    }
                    (name, _) => name,
                },
                point: point(&v["location"])?,
            })
        })
        .collect()
}

fn search_payload(query: &str, origin: Point, radius: f64) -> Value {
    json!({"textQuery":query,"pageSize":20,"locationBias":{"circle":{"center":{"latitude":origin.lat,"longitude":origin.lon},"radius":radius}}})
}

fn nearby_payload(p: Point, types: Option<&[&str]>) -> Value {
    let mut value = json!({"maxResultCount":20,"rankPreference":"DISTANCE","locationRestriction":{"circle":{"center":{"latitude":p.lat,"longitude":p.lon},"radius":2000.0}}});
    if let Some(types) = types {
        value["includedTypes"] = json!(types);
    }
    value
}

fn google_group(kind: &str) -> &'static str {
    if kind.contains("restaurant")
        || matches!(
            kind,
            "cafe" | "coffee_shop" | "bakery" | "bar" | "food_court"
        )
    {
        "Food and drink"
    } else if kind.contains("store") || matches!(kind, "shopping_mall" | "pharmacy" | "bank") {
        "Shopping and services"
    } else if kind.contains("station") || kind.contains("stop") {
        "Transit"
    } else if matches!(
        kind,
        "park" | "tourist_attraction" | "museum" | "historical_place" | "art_gallery"
    ) {
        "Sights and outdoors"
    } else if matches!(kind, "hospital" | "doctor" | "dentist") {
        "Healthcare"
    } else if matches!(kind, "school" | "university" | "library") {
        "Community"
    } else {
        "Other places"
    }
}

fn append_nearby(value: &Value, places: &mut Vec<Place>, seen: &mut HashSet<String>) {
    for v in value["places"].as_array().into_iter().flatten() {
        let (Some(name), Some(location)) = (place_name(v), point(&v["location"])) else {
            continue;
        };
        let id = v["id"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{}:{:.6}:{:.6}", name, location.lat, location.lon));
        if !seen.insert(id) {
            continue;
        }
        let primary = v["primaryType"].as_str().unwrap_or("place");
        places.push(Place {
            name,
            kind: primary.replace('_', " "),
            street: String::new(),
            point: location,
            group: google_group(primary).into(),
            wikipedia: String::new(),
            wikidata: String::new(),
            description: v["formattedAddress"].as_str().unwrap_or("").to_owned(),
        });
    }
}

fn parse_address(value: &Value) -> AddressContext {
    let mut address = AddressContext::default();
    let Some(first) = value["results"].as_array().and_then(|a| a.first()) else {
        return address;
    };
    address.address_point = point(&json!({
        "latitude": first["geometry"]["location"]["lat"],
        "longitude": first["geometry"]["location"]["lng"]
    }));
    for component in first["address_components"].as_array().into_iter().flatten() {
        let types = component["types"].as_array();
        let has = |kind| types.is_some_and(|items| items.iter().any(|v| v == kind));
        let name = component["long_name"].as_str().unwrap_or("").to_owned();
        if has("street_number") {
            address.house = name;
        } else if has("route") {
            address.street = name;
        } else if has("locality") {
            address.city = name;
        } else if has("administrative_area_level_1") {
            address.state = name;
        } else if has("country") {
            address.country = name;
        }
    }
    address
}

fn check_geocode(value: &Value) -> Result<(), String> {
    match value["status"].as_str().unwrap_or("") {
        "OK" | "ZERO_RESULTS" => Ok(()),
        status => Err(format!(
            "Google Geocoding returned {status}: {}",
            value["error_message"]
                .as_str()
                .unwrap_or("check that Geocoding API is enabled for this key")
        )),
    }
}

fn parse_route(value: &Value, destination: SearchResult) -> Result<Route, String> {
    let steps = value["routes"][0]["legs"][0]["steps"]
        .as_array()
        .ok_or("Google returned no route steps")?;
    let mut points: Vec<Point> = Vec::new();
    let mut maneuvers = Vec::new();
    let mut cruise_speeds = Vec::new();
    let mut distance = 0.;
    for step in steps {
        let coords = step["polyline"]["geoJsonLinestring"]["coordinates"]
            .as_array()
            .ok_or("Google route step has no geometry")?;
        let mut step_points = Vec::new();
        for c in coords {
            let p = Point {
                lon: c[0].as_f64().ok_or("Invalid Google route longitude")?,
                lat: c[1].as_f64().ok_or("Invalid Google route latitude")?,
            };
            if !p.valid() {
                return Err("Invalid Google route point".into());
            }
            step_points.push(p);
        }
        if step_points.len() < 2 {
            continue;
        }
        let at = distance;
        let estimated_kmh = step["distanceMeters"]
            .as_f64()
            .zip(
                step["staticDuration"]
                    .as_str()
                    .and_then(|text| text.strip_suffix('s'))
                    .and_then(|text| text.parse::<f64>().ok()),
            )
            .and_then(|(metres, seconds)| {
                (metres > 0. && seconds > 0.).then_some((metres / seconds * 3.6).clamp(15., 115.))
            });
        if let Some(speed) = estimated_kmh {
            cruise_speeds.push((at, speed));
        }
        for p in &step_points {
            if let Some(last) = points.last().copied() {
                let gap = last.distance(*p);
                if gap < 0.01 {
                    continue;
                }
                distance += gap;
            }
            points.push(*p);
        }
        let text = step["navigationInstruction"]["instructions"]
            .as_str()
            .filter(|s| !s.is_empty())
            .unwrap_or(if maneuvers.is_empty() {
                "Head along the route"
            } else {
                "Continue along the route"
            });
        maneuvers.push(Maneuver {
            at,
            text: text.trim_end_matches('.').to_owned(),
            bearing: step_points[0].bearing(step_points[1]),
        });
    }
    if points.len() < 2 {
        return Err("Google route has no usable geometry".into());
    }
    maneuvers.push(Maneuver {
        at: distance,
        text: "Arrive at your destination".into(),
        bearing: maneuvers.last().map_or(0., |m| m.bearing),
    });
    let mut route = Route::new(destination.name, destination.point, points, maneuvers)?;
    route.cruise_speeds = cruise_speeds;
    Ok(route)
}

impl Network {
    pub fn google_search(
        &mut self,
        query: &str,
        origin: Option<Point>,
    ) -> Result<Vec<SearchResult>, String> {
        if let Some((lat, lon)) = query.split_once(',') {
            if let (Ok(lat), Ok(lon)) = (lat.trim().parse(), lon.trim().parse()) {
                let p = Point { lat, lon };
                if p.valid() {
                    return Ok(vec![SearchResult {
                        name: query.into(),
                        point: p,
                    }]);
                }
            }
        }
        let key = key()?;
        if query
            .trim()
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit())
        {
            let value = response_json(
                self.client
                    .get("https://maps.googleapis.com/maps/api/geocode/json")
                    .query(&[("address", query), ("key", &key)]),
                "address search",
            )?;
            check_geocode(&value)?;
            let results = value["results"]
                .as_array()
                .ok_or("Invalid Google address response")?;
            return Ok(results.iter().take(20).filter_map(|v| Some(SearchResult {
                name: v["formatted_address"].as_str()?.to_owned(),
                point: point(&json!({"latitude":v["geometry"]["location"]["lat"],"longitude":v["geometry"]["location"]["lng"]}))?,
            })).collect());
        }
        let explicit_location = query.to_ascii_lowercase().contains(" in ");
        let radii: &[f64] = if origin.is_some() && !explicit_location {
            &SEARCH_RADII
        } else {
            &[50_000.]
        };
        let mut results = Vec::new();
        let mut seen = HashSet::new();
        let mut fallback = Vec::new();
        for &radius in radii {
            let payload = match origin {
                Some(p) => search_payload(query, p, radius),
                None => json!({"textQuery":query,"pageSize":20}),
            };
            let value = response_json(
                self.client
                    .post("https://places.googleapis.com/v1/places:searchText")
                    .header("X-Goog-Api-Key", &key)
                    .header(
                        "X-Goog-FieldMask",
                        "places.id,places.displayName,places.formattedAddress,places.location",
                    )
                    .json(&payload),
                "place search",
            )?;
            let parsed = parse_search(&value);
            if explicit_location || origin.is_none() {
                return Ok(parsed);
            }
            let center = origin.expect("checked above");
            for result in parsed {
                let distance = center.distance(result.point);
                if distance <= radius {
                    let id = format!(
                        "{}:{:.6}:{:.6}",
                        result.name, result.point.lat, result.point.lon
                    );
                    if seen.insert(id) {
                        results.push(result);
                    }
                } else if radius == SEARCH_RADII[SEARCH_RADII.len() - 1] {
                    fallback.push(result);
                }
            }
            if results.len() >= 10 {
                break;
            }
        }
        if results.is_empty() {
            results = fallback;
        }
        if let Some(center) = origin {
            results.sort_by(|a, b| {
                center
                    .distance(a.point)
                    .total_cmp(&center.distance(b.point))
            });
        }
        Ok(results)
    }

    pub fn google_reverse(&mut self, p: Point) -> Result<AddressContext, String> {
        let key = key()?;
        let value = response_json(
            self.client
                .get("https://maps.googleapis.com/maps/api/geocode/json")
                .query(&[
                    ("latlng", format!("{:.7},{:.7}", p.lat, p.lon)),
                    ("key", key),
                ]),
            "address lookup",
        )?;
        check_geocode(&value)?;
        Ok(parse_address(&value))
    }

    pub fn google_area(&mut self, result: SearchResult) -> Result<Area, String> {
        let key = key()?;
        let p = result.point;
        let fetch = |types: Option<&[&str]>| {
            response_json(self.client
            .post("https://places.googleapis.com/v1/places:searchNearby")
            .header("X-Goog-Api-Key", &key)
            .header("X-Goog-FieldMask", "places.id,places.displayName,places.formattedAddress,places.location,places.primaryType")
            .json(&nearby_payload(p, types)), "neighborhood lookup")
        };
        let value = fetch(None)?;
        let mut places = Vec::new();
        let mut seen = HashSet::new();
        append_nearby(&value, &mut places, &mut seen);
        if value["places"].as_array().is_some_and(|v| v.len() == 20) {
            for types in NEARBY_TYPES {
                if let Ok(extra) = fetch(Some(types)) {
                    append_nearby(&extra, &mut places, &mut seen);
                }
            }
        }
        places.sort_by(|a, b| p.distance(a.point).total_cmp(&p.distance(b.point)));
        let street = self.google_reverse(p)?.street;
        Ok(Area {
            name: result.name,
            version: 10,
            center: p,
            radius: 2000.,
            roads: vec![],
            places,
            signalized_crossings: vec![],
            road_events: vec![],
            google_street: street,
        })
    }

    pub fn google_route(
        &mut self,
        start: Point,
        destination: SearchResult,
        driving: bool,
        preference: HighwayPreference,
    ) -> Result<Route, String> {
        let key = key()?;
        let mut payload = json!({
            "origin":{"location":{"latLng":{"latitude":start.lat,"longitude":start.lon}}},
            "destination":{"location":{"latLng":{"latitude":destination.point.lat,"longitude":destination.point.lon}}},
            "travelMode": if driving { "DRIVE" } else { "WALK" },
            "polylineQuality":"HIGH_QUALITY",
            "polylineEncoding":"GEO_JSON_LINESTRING",
            "languageCode":"en-US"
        });
        if driving {
            payload["routingPreference"] = json!("TRAFFIC_UNAWARE");
            payload["routeModifiers"] =
                json!({"avoidHighways": preference != HighwayPreference::Fastest});
        }
        let value = response_json(
            self.client
                .post("https://routes.googleapis.com/directions/v2:computeRoutes")
                .header("X-Goog-Api-Key", key)
                .header(
                    "X-Goog-FieldMask",
                    "routes.legs.steps.polyline,routes.legs.steps.navigationInstruction,routes.legs.steps.distanceMeters,routes.legs.steps.staticDuration",
                )
                .json(&payload),
            "route",
        )?;
        parse_route(&value, destination)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_search_uses_virtual_position_and_expanding_radius() {
        let origin = Point {
            lat: 47.6,
            lon: -122.3,
        };
        for radius in SEARCH_RADII {
            let request = search_payload("Mcdonalds", origin, radius);
            assert_eq!(
                request["locationBias"]["circle"]["center"]["latitude"],
                47.6
            );
            assert_eq!(request["locationBias"]["circle"]["radius"], radius);
        }
        assert_eq!(SEARCH_RADII, [1000., 5000., 25000., 50000.]);
    }

    #[test]
    fn nearby_batches_add_distinct_places_and_categories() {
        let origin = Point {
            lat: 47.6,
            lon: -122.3,
        };
        let first = json!({"places":[{"id":"one","displayName":{"text":"One"},"location":{"latitude":47.6,"longitude":-122.3},"primaryType":"restaurant"}]});
        let second = json!({"places":[
            {"id":"one","displayName":{"text":"One"},"location":{"latitude":47.6,"longitude":-122.3},"primaryType":"restaurant"},
            {"id":"two","displayName":{"text":"Two"},"location":{"latitude":47.601,"longitude":-122.3},"primaryType":"coffee_shop"}
        ]});
        let mut places = Vec::new();
        let mut seen = HashSet::new();
        append_nearby(&first, &mut places, &mut seen);
        append_nearby(&second, &mut places, &mut seen);
        assert_eq!(places.len(), 2);
        assert_eq!(places[1].category(), "Food and drink");
        assert_eq!(
            nearby_payload(origin, Some(NEARBY_TYPES[0]))["includedTypes"][0],
            "restaurant"
        );
    }
    #[test]
    fn google_steps_keep_turns_and_geometry() {
        let start = Point {
            lat: 47.0,
            lon: -122.0,
        };
        let turn = start.walk(0., 100.);
        let end = turn.walk(90., 100.);
        let value = json!({"routes":[{"legs":[{"steps":[
            {"polyline":{"geoJsonLinestring":{"coordinates":[[start.lon,start.lat],[turn.lon,turn.lat]]}},"navigationInstruction":{"instructions":"Head north on Main Street"},"distanceMeters":100,"staticDuration":"5s"},
            {"polyline":{"geoJsonLinestring":{"coordinates":[[turn.lon,turn.lat],[end.lon,end.lat]]}},"navigationInstruction":{"instructions":"Turn right onto Pine Street"},"distanceMeters":100,"staticDuration":"10s"}
        ]}]}]});
        let route = parse_route(
            &value,
            SearchResult {
                name: "Test".into(),
                point: end,
            },
        )
        .unwrap();
        assert_eq!(route.maneuvers[1].text, "Turn right onto Pine Street");
        assert!(route.maneuvers[1].at > 90.);
        assert_eq!(route.cruise_speed(0.), Some(72.));
        assert_eq!(route.cruise_speed(150.), Some(36.));
    }
}
