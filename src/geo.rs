use serde::{Deserialize, Serialize};

const R: f64 = 6_371_000.0;
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct Point {
    pub lat: f64,
    pub lon: f64,
}
impl Point {
    pub fn valid(self) -> bool {
        self.lat.is_finite()
            && self.lon.is_finite()
            && self.lat.abs() <= 85.0
            && self.lon.abs() <= 180.0
    }
    pub fn distance(self, b: Self) -> f64 {
        let a = ((b.lat - self.lat).to_radians() / 2.0).sin().powi(2)
            + self.lat.to_radians().cos()
                * b.lat.to_radians().cos()
                * ((b.lon - self.lon).to_radians() / 2.0).sin().powi(2);
        2.0 * R * a.clamp(0.0, 1.0).sqrt().asin()
    }
    pub fn bearing(self, b: Self) -> f64 {
        let d = (b.lon - self.lon).to_radians();
        (d.sin() * b.lat.to_radians().cos())
            .atan2(
                self.lat.to_radians().cos() * b.lat.to_radians().sin()
                    - self.lat.to_radians().sin() * b.lat.to_radians().cos() * d.cos(),
            )
            .to_degrees()
            .rem_euclid(360.0)
    }
    pub fn walk(self, bearing: f64, metres: f64) -> Self {
        let (lat, lon, angle, d) = (
            self.lat.to_radians(),
            self.lon.to_radians(),
            bearing.to_radians(),
            metres / R,
        );
        let new_lat = (lat.sin() * d.cos() + lat.cos() * d.sin() * angle.cos()).asin();
        let new_lon =
            lon + (angle.sin() * d.sin() * lat.cos()).atan2(d.cos() - lat.sin() * new_lat.sin());
        Self {
            lat: new_lat.to_degrees(),
            lon: (new_lon.to_degrees() + 180.0).rem_euclid(360.0) - 180.0,
        }
    }
}
pub fn clock(bearing: f64, heading: f64) -> u32 {
    let h = ((bearing - heading).rem_euclid(360.0) / 30.0).round() as u32 % 12;
    if h == 0 {
        12
    } else {
        h
    }
}
pub fn compass(heading: f64) -> &'static str {
    [
        "North",
        "North East",
        "East",
        "South East",
        "South",
        "South West",
        "West",
        "North West",
    ][((heading.rem_euclid(360.0) / 45.0).round() as usize) % 8]
}
pub fn project(p: Point, a: Point, b: Point) -> Point {
    let scale = p.lat.to_radians().cos();
    let delta = |x: f64, y: f64| (x - y + 180.0).rem_euclid(360.0) - 180.0;
    let (ax, ay) = (delta(a.lon, p.lon) * scale, a.lat - p.lat);
    let (bx, by) = (delta(b.lon, p.lon) * scale, b.lat - p.lat);
    let (dx, dy) = (bx - ax, by - ay);
    let denom = dx * dx + dy * dy;
    let t = if denom < 1e-20 {
        0.0
    } else {
        (-(ax * dx + ay * dy) / denom).clamp(0.0, 1.0)
    };
    Point {
        lat: p.lat + ay + t * dy,
        lon: (p.lon + (ax + t * dx) / scale + 180.0).rem_euclid(360.0) - 180.0,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn walk_distance_and_heading() {
        let p = Point {
            lat: 47.6,
            lon: -122.3,
        };
        for h in [0., 90., 180., 270.] {
            let b = p.walk(h, 50.);
            assert!((p.distance(b) - 50.).abs() < 0.001);
            assert!((p.bearing(b) - h).abs() < 0.001);
        }
    }
    #[test]
    fn relative_clock() {
        assert_eq!(clock(90., 90.), 12);
        assert_eq!(clock(180., 90.), 3);
        assert_eq!(clock(0., 90.), 9);
        assert_eq!(clock(359., 0.), 12);
    }
    #[test]
    fn segment_not_endpoint() {
        let p = Point {
            lat: 0.001,
            lon: 0.005,
        };
        let q = project(p, Point { lat: 0., lon: 0. }, Point { lat: 0., lon: 0.01 });
        assert!(q.lat.abs() < 1e-9);
        assert!((q.lon - 0.005).abs() < 1e-9);
    }
    #[test]
    fn date_line() {
        let p = Point {
            lat: 0.,
            lon: 179.9999,
        };
        let q = p.walk(90., 100.);
        assert!(q.valid());
        assert!((p.distance(q) - 100.).abs() < 0.001);
    }
}
