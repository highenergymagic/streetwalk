use crate::geo::Point;
use windows::Devices::Geolocation::{GeolocationAccessStatus, Geolocator};

// RequestAccessAsync must start on the foreground UI thread. Waiting for the
// result, and for the one-time fix, is safe to do on a worker thread.
pub fn request() -> Result<windows_future::IAsyncOperation<GeolocationAccessStatus>, String> {
    Geolocator::RequestAccessAsync().map_err(|e| format!("Could not request PC location: {e}"))
}

pub fn finish(
    access: windows_future::IAsyncOperation<GeolocationAccessStatus>,
) -> Result<Point, String> {
    if access.join().map_err(|e| e.to_string())? != GeolocationAccessStatus::Allowed {
        return Err("PC location is unavailable or permission was denied. Check Windows Settings > Privacy & security > Location.".into());
    }
    let coordinate = Geolocator::new()
        .and_then(|g| g.GetGeopositionAsync())
        .and_then(|op| op.join())
        .and_then(|position| position.Coordinate())
        .map_err(|e| format!("Could not get the PC's location: {e}"))?;
    let point = Point {
        lat: coordinate.Latitude().map_err(|e| e.to_string())?,
        lon: coordinate.Longitude().map_err(|e| e.to_string())?,
    };
    if !point.valid() {
        return Err("Windows returned an invalid location.".into());
    }
    Ok(point)
}
