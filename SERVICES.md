# Streetwalk services, privacy and build details

This file describes the current network calls and local files. The [user guide](USERGUIDE.md) covers operation; the [developer README](README.md) covers the code and releases.

## Map and route providers

| Feature | OpenStreetMap mode | Google Maps mode |
| --- | --- | --- |
| Place search | [Photon](https://github.com/komoot/photon) | Places API (New) |
| Neighborhood and POIs | [Soundscape GeoJSON tiles](https://github.com/soundscape-community/soundscape/blob/main/docs/Services.md), with [Overpass](https://wiki.openstreetmap.org/wiki/Overpass_API) fallback | Places API (New) nearby search |
| Address lookup | Photon reverse lookup plus mapped address detail | Geocoding API |
| Walking route | [FOSSGIS pedestrian OSRM](https://routing.openstreetmap.de/about.html) | Routes API |
| Fastest drive | FOSSGIS car OSRM | Routes API |
| Smaller-road or avoid-motorway drive | Public Valhalla route service | Routes API with avoid-highways bias |
| Place context | Linked Wikipedia/Wikidata or OSM description where available | Google place type and formatted address |

OpenStreetMap neighborhoods use zoom-16 tiles cached locally. If tiles fail, Streetwalk tries Overpass through VK Maps and then Private.coffee, unless `STREETWALK_OVERPASS_URL` overrides them. Photon searches use the virtual position as a ranking bias and cache results by query and nearby origin. Public servers have capacity limits; a failed request leaves the last map in place. Map and route downloads have a separate worker so they do not hold up address and traffic requests. Service responses have size limits. The UI attributes OpenStreetMap data and offers a Fix the map button. Google mode shows Google attribution. Google places remain available when a reverse address lookup fails. Google places, address and route responses are not saved for offline reuse.

## Weather and traffic

[Open-Meteo](https://open-meteo.com/en/docs) receives queried virtual coordinates and returns current weather, including temperature, conditions, precipitation and wind. A drive checks roughly every 5 km or ten minutes. Its [Elevation API](https://open-meteo.com/en/docs/elevation-api) receives batches of up to 100 points along the active route, covering roughly the next 8 km and refreshing as the drive advances. The 90 m Copernicus terrain model estimates grade; it does not measure the exact road surface, bridges or tunnels. Weather and elevation readings are kept in memory for the session. Review [Open-Meteo's terms](https://open-meteo.com/en/terms) before commercial use. Elevation data attribution: Copernicus DEM GLO-90 via Open-Meteo.

When enabled with a key, TomTom receives sampled virtual road coordinates for [Traffic Flow](https://docs.tomtom.com/traffic-api/documentation/tomtom-maps/v1/traffic-flow/flow-segment-data) and an area around the virtual position for [Traffic Incidents](https://docs.tomtom.com/traffic-api/documentation/tomtom-orbis-maps/v2/traffic-incidents/incident-details). Flow is refreshed roughly every 1.5 km or two minutes, incidents every 5 km or five minutes. Congestion is applied only when the returned road segment matches the virtual route's position and heading; a slowdown also raises synthetic surrounding-car activity, combined with local mapped place density. A synthetic lead vehicle adds correlated speed changes and following gaps in congested traffic. TomTom Flow supplies speed and confidence, not vehicle counts or tracks; reported incidents near a route may be on a parallel road. A reported closure can pause a drive. Streetwalk does not query actual red/yellow/green signal phase. The optional mapped-crossing wait is simulated.

The optional Windows PC-location choice requests one position at startup or when Ctrl+L is pressed. It does not track later PC movement. That position is sent to the selected map provider when Streetwalk loads its neighborhood. In all modes, the service operator sees your IP address and the virtual coordinates, search text or route endpoints that you query. Place context requests send the linked article or item ID to Wikimedia. Live weather and traffic use the present real-world time even during a virtual drive.

## Local files and keys

By default, writable files go in `data` beside the executable; `STREETWALK_DATA` changes this directory. OpenStreetMap neighborhoods, searches and routes can be reused offline from there. Position, heading, walking step, turn increment, options and bookmarks are saved. An active OpenStreetMap walking route is restored after normal closing. New uncached real-world areas and routes need internet. The included fictional demo works offline.

A TomTom key can be read from `tomtom_key.txt` beside the executable, the older `data/tomtom_key.txt`, or `STREETWALK_TOMTOM_KEY`. A Google key can be read from `google_key.txt` beside the executable or `STREETWALK_GOOGLE_KEY`. Environment variables take precedence. Portable packages exclude both keys by default. `package.ps1 -IncludeTomTomKey` explicitly includes the configured TomTom key and grants recipients use of its quota. The package script never adds a Google key.

## Service overrides

Set these environment variables before launching:

| Variable | Purpose and default |
| --- | --- |
| `STREETWALK_PHOTON_URL` | Photon search endpoint; default `https://photon.komoot.io/api/`. Reverse lookup uses `/reverse` at that server root. |
| `STREETWALK_SOUNDSCAPE_URL` | Soundscape tile root; default `https://tiles.soundscape.services`. |
| `STREETWALK_OVERPASS_URL` | One Overpass interpreter endpoint instead of the default fallback sequence. |
| `STREETWALK_ROUTER_URL` | Pedestrian OSRM endpoint; default `https://routing.openstreetmap.de/routed-foot/route/v1/foot`. |
| `STREETWALK_DRIVING_ROUTER_URL` | Car OSRM endpoint; default `https://routing.openstreetmap.de/routed-car/route/v1/driving`. |
| `STREETWALK_VALHALLA_URL` | Valhalla route endpoint; default `https://valhalla1.openstreetmap.de/route`. |
| `STREETWALK_WEATHER_URL` | Open-Meteo-compatible endpoint; default `https://api.open-meteo.com/v1/forecast`. |
| `STREETWALK_ELEVATION_URL` | Open-Meteo-compatible elevation endpoint; default `https://api.open-meteo.com/v1/elevation`. |
| `STREETWALK_TOMTOM_FLOW_URL` | TomTom-compatible Flow Segment Data endpoint; default `https://api.tomtom.com/traffic/services/4/flowSegmentData/absolute/18/json`. |
| `STREETWALK_TOMTOM_INCIDENTS_URL` | TomTom-compatible Orbis Incident Details endpoint; default `https://api.tomtom.com/maps/orbis/traffic/incidents/details`. |
| `STREETWALK_DATA` | Writable cache and settings directory. |

## Build and verify

Windows, Rust and a matching native linker are required. The script uses `tools/w64devkit` if present and does not permanently alter the system PATH.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\build.ps1
```

`build.ps1` checks formatting, runs the unit tests, builds the release executable and assembles `dist/Streetwalk`. With the official NVDA controller archive extracted under `dist/nvda-controller`, `package.ps1` creates a timestamped portable folder and ZIP under `dist`; a TomTom key is only required with `-IncludeTomTomKey`. The portable package includes source, lockfile, the user guide, notices and an empty data folder. GitHub Actions builds a key-free portable artifact on main, pull requests and manual runs, and publishes it on a matching version tag. Locally downloaded research PDFs and the original Freesound preview stay in `research/leaf2017/sources` and are excluded from the portable package.

For explicit network smoke tests, use `cargo test live_search_download_and_cache -- --ignored --nocapture`. They contact public services and write under `target/live-smoke`. With NVDA running, `cargo test --test nvda -- --ignored --nocapture` checks the controller DLL connection without speaking. Offline unit tests cover map parsing, navigation, speech, audio and route behavior; they do not verify perceived realism or every keyboard-focus path in a live Windows session.

## Attribution and limits

OpenStreetMap data © [OpenStreetMap contributors](https://www.openstreetmap.org/copyright), under ODbL. Google mode data © Google Maps. Soundscape audio, MIT KEMAR responses, the CC0 LEAF texture source, the NVDA controller DLL and Rust dependencies have notices or licenses in the portable folder. The [audio model notes](EV-AUDIO-MODEL.md) and [LEAF research directory](research/leaf2017/README.md) describe what is measured and what remains synthetic.

Streetwalk is for virtual exploration. Free walking can cross obstacles, routing endpoints may snap to nearby roads, POIs may be placed at approximate geometry centers, and map data can be incomplete or stale. No route or spoken speed is a substitute for physical navigation or a legal speed-limit source.
