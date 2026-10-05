# Streetwalk

Streetwalk is a keyboard-first Windows map explorer for virtual walking and driving. It speaks streets, crossings, places, directions and route instructions through NVDA. OpenStreetMap mode works without an account; Google Maps mode is optional and needs your own key. It is an exploration tool, not guidance for travel in the physical world.

## Start here

1. Run `streetwalk.exe` with NVDA running. Keep `nvdaControllerClient.dll` beside it. The folder must be writable so Streetwalk can save `data`.
2. At first launch, choose whether to start at this PC's Windows location. This requests a fix once at startup; it does not follow the PC afterward. Choose No to use the saved virtual position or the fictional offline demo.
3. Press **Ctrl+F**, enter a town, place, full street address, or `latitude, longitude`, then press Enter. Use Up/Down to choose a result and Enter to jump there. **Escape** focuses Virtual walk.
4. Use the arrow keys to walk north, east, south or west. **Space** gives a short location summary; **Shift+Space** looks up a fuller address and nearby places. **F1** opens this guide in your default browser.

The opening screen has buttons for My location, Around me, Ahead of me, Nearby, Search, Route and Saved. Tab and Shift+Tab move through controls, including out of the announcement transcript. The transcript holds the 40 latest announcements and remains selectable without NVDA. Single-letter movement and exploration keys work when **Virtual walk** has focus; global Ctrl shortcuts and F1/F2 work elsewhere.

## Explore on foot

- **Arrow keys** move one step in a compass direction and face that direction. Holding a key does not repeat movement. **W/X** move forward/backward without changing heading. **Q/E** turn by the configured increment; **Shift+Q/E** turn one degree. The Heading degrees/Face and Turn degrees/Apply controls accept exact values.
- **Plus/minus** or **] / [** change step length among 1, 5, 10, 25, 50 and 100 metres. **Backspace** undoes movement or a jump. **H** returns to the loaded neighborhood's starting point. **C** reads coordinates.
- **Space** describes the current street and heading. **Shift+Space** asks the selected map provider for an address, then includes available city, state and country, heading, and up to two nearby named places with clock directions. It may take a moment while a network request completes.
- **O** opens Around me, with nearby places by direction. **A** opens Ahead of me. **N** opens a category picker; choose a category with Up/Down and Enter. **P** opens all nearby POIs nearest first. **L** speaks the ten nearest POIs as separate entries. Standalone house numbers appear under Nearby → Addresses rather than in the P list.
- Lists show ten entries per page. Up/Down continue across page edges; Page Up/Down and the Previous/Next buttons change pages. The label reports the visible range. Enter on a POI jumps there; **Ctrl+Enter** or **G** routes there. Backspace returns from a category's places to categories, or from other place lists to walking.
- **I** or **Ctrl+I** requests more information about the selected or nearest notable place. Descriptions may come from mapped Wikipedia/Wikidata links or OpenStreetMap text. Google place details are shorter.
- **K** toggles an audio beacon toward a selected place or active route destination. **V** opens Street Preview: choose a road at the next mapped decision point, then Enter advances to its next junction or end. **S** snaps to a mapped street within 100 metres. Street Preview and snapping need OpenStreetMap road geometry and are unavailable in Google mode.
- **B** saves the current position and neighborhood for later offline use; **J** browses saved positions. **R** asks for a map coverage check. **M** toggles automatic speech and spatial cues.

Free walking is virtual: arrow/W/X movement can cross buildings or water. Road Preview, snapping and pedestrian routes use map data but do not establish physical accessibility or safety.

## Search, addresses and routes

**Ctrl+F** searches for a starting place and jumps to the selected result. **Ctrl+D** searches for a destination; Enter on its result plans a walking route from your current virtual position. In either result list, **Ctrl+Enter** or the Route button plans a walking route, while the Jump button moves to a result. Searches can return up to 50 results and are paged. For an address, include house number, street and town; inspect the full result label before jumping.

The **Route** button opens the active walking route or destination search. Its directions are paged. **F** follows one configured walking step along the route, stopping at a turn. **T** reads the next instruction and **Shift+T** reads the route summary. **Shift+G** recalculates from your current position; **U** cancels. You can also walk manually. Streetwalk announces leaving and rejoining a route. Route endpoints may be street access points some distance from your exact requested place, and arrival reports a remaining gap when present.

## Virtual Drive

Select a destination in a place or search list and press **Ctrl+Shift+Enter** or the **Drive tour** button. A real mapped starting area and a road route are required. The car starts at rest, waits briefly for the simulated driver's decision, then builds acceleration gradually. Each drive samples a consistent driving style, varying pace, acceleration, coasting, braking, and corner speed; gentle turns may be taken with more speed. It slows for bends in the route geometry as well as sharper named turns and mapped stops. Sampled terrain grade changes motor load and uphill acceleration when elevation is available. With EV cabin sound enabled, the indicator has a stalk clunk on engagement and release plus unevenly spaced ticks before the corresponding maneuver. **D** pauses or resumes; **U** ends the drive at its current virtual position. Manual walking also ends it. The drive advances only while Streetwalk's walking view is focused.

The drive speaks advance and immediate turn instructions, road continuations, changed streets and periodic location context. Nearby named POIs receive a spatial cue and a short announcement. Notable places may also get a longer description. Address checks are periodic, about every kilometre or 75 seconds while moving; they are not spoken on every position update. After an “On [street]” announcement, POI speech waits at least 2.5 seconds. Streetwalk queues NVDA announcements in order; **F2** stops the current announcement and clears Streetwalk speech queued in NVDA.

**Z** opens Map and driving options. Use Up/Down to select a setting, Enter or Right to increase it, and Left to decrease it. Options are saved and include:

- **Route:** Fastest, Prefer smaller roads or Avoid motorways. Smaller-road preferences can still use a highway. Avoiding motorways can cause a long detour. A new preference applies to new drives.
- **Simulated signal wait:** off by default; 5 to 30 seconds at mapped signalized crossings in OpenStreetMap mode. This is a simulated wait, not a live traffic-light phase.
- **Live traffic:** on by default when a TomTom key is configured. Matching TomTom road flow can lower virtual speed and influence the estimated number of audible surrounding cars; a reported closure can pause the drive. In congestion, a simulated lead car adds gradual following and stop–go variation. TomTom supplies the road's estimated flow speed, not the lead car's position or a vehicle count. Nearby incident reports may be spoken. Traffic reflects current real-world conditions, not the virtual time of travel.
- **Fallback speeds:** built-up 35, suburban 50, rural 70 and open road 95 km/h by default, adjustable in 5 km/h steps. OpenStreetMap `maxspeed` tags are used where available; missing limits are estimated. Google mode instead derives speed from route-step timing and uses the suburban fallback when timing is absent. Neither estimate is a posted speed limit.
- **Map and route source:** OpenStreetMap or Google Maps. Switching reloads the current virtual position and ends the current route/drive.
- **EV cabin sound and volume:** on by default at 70 percent. The stereo model includes tires, mapped road-surface changes and traffic-calming impacts where OSM tags exist, enclosed wind, gentle dashboard-vent airflow and faint distant traffic that remain audible at a stop, brief muffled passing-car whooshes, diffuse indicator ticks and stalk clunks, wet-road spray and intermittent wipers in rain, and a centered LEAF-style motor whine. The motor drive fades over about 1.5 seconds after an acceleration or braking event. Headphones work best. The [audio model notes](EV-AUDIO-MODEL.md) explain sources and limits.

The cabin room tone continues while the virtual car is stopped. Sound and driving pause when Streetwalk loses focus. The lead car and passing-car audio are simulated; no service supplies individual vehicle tracks. The sound estimates roughly 5–26 audible car passes per minute from local mapped place density and reported TomTom slowdown, with variation between passes.

## Weather and automatic announcements

**Y** reads modeled weather at the virtual position. A drive checks again after roughly 5 km or ten minutes and speaks meaningful changes. Weather is from Open-Meteo at the present real-world time, not the simulated time of day.

Ordinary walking steps do not repeat your address. Changed streets, intersections and mapped crossings can be announced promptly; named POIs are spaced out. After a long quiet stretch, Streetwalk gives a short orientation. Soundscape's POI, crossing safety and resume-movement cues play through stereo audio when available. They have left/right positioning relative to virtual heading, with no head tracking. Audio and speech stop when Streetwalk is not foreground or Windows is on a secure desktop.

Real OpenStreetMap neighborhoods download automatically near a coverage edge. Downloads run in the background; you can keep walking while one finishes. **R** requests a check. Failed requests leave your last map available and retry later. Previously visited OpenStreetMap areas, searches and routes may be available from the local cache; new requests need internet. Google map responses are not kept as an offline cache. The fictional demo works offline.

## Google Maps mode

Google mode uses Places for place search and nearby POIs, Geocoding for addresses, and Routes for walking and driving. Free arrow/W/X walking is available immediately; it is not constrained to a road graph. Nearby places and street context refresh as you move. **Ctrl+D** broadens a destination search around your virtual position when local matches are scarce. TomTom traffic and Open-Meteo weather remain available. Street Preview and S snapping are unavailable because these Google APIs do not provide Streetwalk with a complete road graph. Both smaller-road route preferences request Google's avoid-highways bias; neither guarantees a highway-free route. Google mode does not provide posted speed-limit or signalized-crossing data to Streetwalk.

To enable it:

1. In the [Google Maps Platform console](https://console.cloud.google.com/google/maps-apis/overview), create a project and billing account.
2. Enable Routes API, Places API (New), and Geocoding API. Create an API key restricted to those APIs. Set quotas that fit your budget.
3. Put the key alone in `google_key.txt` beside `streetwalk.exe`, or set `STREETWALK_GOOGLE_KEY` before starting.
4. Press **Z**, select Map and route source, then Enter or Right to select Google Maps.

Google requests may be billable. Check [current pricing and monthly no-charge caps](https://developers.google.com/maps/billing-and-pricing/pricing) in your account. Streetwalk does not persist Google places, addresses or routes as an offline cache. Keep your key private when sharing a portable build.

## Shortcuts and help

| Key | Action |
| --- | --- |
| Ctrl+L | Request this PC's Windows location now and use it at future starts |
| Ctrl+Shift+L | Stop using PC location at future starts |
| Escape | Return to and focus Virtual walk |
| F1 | Open this guide in the default browser |
| F2 | Stop current NVDA speech and clear Streetwalk's speech queue |
| Tab / Shift+Tab | Move between controls, including out of the transcript |

Clock directions are relative to virtual heading: 12 ahead, 3 right, 6 behind and 9 left. The HTML help page has headings for screen-reader navigation. It is regenerated from the `README.md` beside the executable each time F1 is pressed.

## Files, services and limitations

The portable folder includes the executable, NVDA controller DLL, the LEAF cabin texture and its notice, documentation, source, build scripts, and an empty `data` folder. API keys are excluded by default. To include the configured TomTom key intentionally, run `./package.ps1 -IncludeTomTomKey`; anyone receiving that package can use the key's quota. Your Google key is never packaged automatically.

Streetwalk's original code is [MIT licensed](LICENSE). Bundled sounds, HRTF measurements, the LEAF texture and the optional NVDA DLL keep their [separate terms and notices](THIRD_PARTY.md). Read [services, privacy and build details](SERVICES.md) for providers, environment variables, cache behavior and build commands. Read the [EV audio model](EV-AUDIO-MODEL.md) and [LEAF research notes](research/leaf2017/README.md) for audio provenance.

Place coverage and addresses vary with contributed or provider data. Streetwalk has no indoor floors, country-size offline map import, or independent SAPI voice. Speech requires NVDA. Free walking and virtual driving are for exploration; they cannot identify real-world obstacles, legal speed limits, safe crossings or current road conditions with enough certainty for physical navigation.
