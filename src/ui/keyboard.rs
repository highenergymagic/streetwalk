use super::*;

impl App {
    pub(super) fn key(&mut self, key: u16, shift: bool) {
        match key {
            VK_UP | VK_RIGHT | VK_DOWN | VK_LEFT => {
                self.walking_step();
                let heading = match key {
                    VK_UP => 0.,
                    VK_RIGHT => 90.,
                    VK_DOWN => 180.,
                    _ => 270.,
                };
                self.move_to(
                    self.point
                        .walk(heading, [1., 5., 10., 25., 50., 100.][self.step]),
                    heading,
                );
            }
            VK_OEM_PLUS | VK_ADD | VK_OEM_6 => {
                self.step = (self.step + 1).min(5);
                self.announce(&format!(
                    "Step {} metres",
                    [1, 5, 10, 25, 50, 100][self.step]
                ));
            }
            VK_OEM_MINUS | VK_SUBTRACT | VK_OEM_4 => {
                self.step = self.step.saturating_sub(1);
                self.announce(&format!(
                    "Step {} metres",
                    [1, 5, 10, 25, 50, 100][self.step]
                ));
            }
            VK_SPACE => {
                if shift {
                    self.where_am_i();
                } else {
                    self.describe();
                }
            }
            0x51 | 0x45 => {
                let turn = if shift { 1. } else { self.turn };
                self.heading =
                    (self.heading + if key == 0x51 { -turn } else { turn }).rem_euclid(360.);
                self.audio.update(self.point, self.heading, unsafe {
                    GetForegroundWindow() == self.hwnd
                });
                unsafe {
                    text(self.heading_input, &format!("{:.0}", self.heading));
                }
                self.announce(&format!(
                    "{} {:.0} degrees. Facing {} at {:.0} degrees",
                    if key == 0x51 { "Left" } else { "Right" },
                    turn,
                    crate::geo::compass(self.heading),
                    self.heading
                ));
            }
            0x57 | 0x58 => {
                self.walking_step();
                let bearing = if key == 0x58 {
                    self.heading + 180.
                } else {
                    self.heading
                };
                self.move_to(
                    self.point
                        .walk(bearing, [1., 5., 10., 25., 50., 100.][self.step]),
                    self.heading,
                );
            }
            0x47 => {
                if shift {
                    if let Some(g) = &self.route {
                        self.plan(SearchResult {
                            name: g.route.name.clone(),
                            point: g.route.destination,
                        });
                    } else {
                        self.announce("No route to recalculate.");
                    }
                } else {
                    self.show_all_places();
                    self.announce("Select a POI, then press G or Ctrl+Enter to route there.");
                }
            }
            0x46 => {
                if let Some(g) = &self.route {
                    let (p, h) = g.advance(self.point, [1., 5., 10., 25., 50., 100.][self.step]);
                    self.walking_step();
                    self.move_to(p, h);
                } else {
                    self.announce("No active route. Ctrl+D finds a destination.");
                }
            }
            0x54 => {
                if let Some(g) = &self.route {
                    self.announce(&if shift { g.route.summary() } else { g.next() });
                } else {
                    self.announce("No active route.");
                }
            }
            0x55 => {
                if self.drive.take().is_some() {
                    self.persist();
                    self.announce("Virtual drive ended. You can explore from here.");
                    return;
                }
                if self
                    .route
                    .as_ref()
                    .is_some_and(|g| self.audio.target() == Some(g.route.destination))
                {
                    self.audio.stop();
                }
                self.route = None;
                self.route_generation += 1;
                self.show_view(View::Explore);
                unsafe { SetFocus(self.walk) };
                self.announce("Route cancelled.");
            }
            0x50 => self.show_all_places(),
            0x49 => self.place_info(),
            0x59 => self.request_weather(true),
            0x5A => self.show_settings(),
            0x44 => {
                if let Some(drive) = &mut self.drive {
                    drive.paused = !drive.paused;
                    drive.paused_by_closure = false;
                    drive.last_tick = Instant::now();
                    let paused = drive.paused;
                    self.announce(if paused {
                        "Virtual drive paused."
                    } else {
                        "Virtual drive resumed."
                    });
                } else {
                    self.announce(
                        "Select a destination in a place or search list, then choose Drive tour.",
                    );
                }
            }
            0x4B => {
                if self.muted {
                    self.announce("Sound is muted. Press M to turn speech and spatial cues on.");
                    return;
                }
                let listed = if matches!(self.view, View::Nearby | View::Preview)
                    && matches!(
                        self.browse_mode,
                        BrowseMode::Places
                            | BrowseMode::AllPlaces
                            | BrowseMode::Explore
                            | BrowseMode::Preview
                    ) {
                    self.selected_choice_index()
                        .and_then(|i| self.choices.get(i))
                        .map(|c| (c.point, c.name.clone()))
                } else {
                    None
                };
                let target = listed.or_else(|| {
                    self.route
                        .as_ref()
                        .map(|g| (g.route.destination, g.route.name.clone()))
                });
                if let Some((point, name)) = target {
                    if !self.audio.available() {
                        self.announce("No audio output device available for a beacon.");
                    } else if self.audio.toggle_beacon(point, self.point, self.heading) {
                        self.announce(&format!(
                            "Audio beacon on: {}. Press K again to stop.",
                            name
                        ));
                    } else {
                        self.announce("Audio beacon off.");
                    }
                } else if self.audio.target().is_some() {
                    self.audio.stop();
                    self.announce("Audio beacon off.");
                } else {
                    self.announce("Choose a nearby place or start a route, then press K for its audio beacon.");
                }
            }
            0x4E => self.show_categories(),
            0x56 => self.show_preview(),
            0x4F => self.explore(false),
            0x41 => self.explore(true),
            VK_RETURN => self.show_all_places(),
            0x53 => {
                if let Some((_, q, d)) = self.area.nearest_road(self.point) {
                    if d <= 100. {
                        self.move_to(q, self.heading);
                    } else {
                        self.announce("No mapped street within 100 metres.");
                    }
                } else {
                    self.announce("No mapped streets in this neighborhood.");
                }
            }
            0x48 => self.move_to(self.area.center, self.heading),
            VK_BACK => {
                if let Some((p, h)) = self.history.pop() {
                    self.point = p;
                    self.heading = h;
                    if let Some(g) = &mut self.route {
                        let route = g.route.clone();
                        *g = Guidance::new(route);
                    }
                    self.passive_update();
                    self.ensure_coverage(false);
                } else {
                    self.announce("No earlier position.");
                }
            }
            0x43 => self.announce(&format!(
                "Latitude {:.6}, longitude {:.6}",
                self.point.lat, self.point.lon
            )),
            0x4C => {
                let lines: Vec<_> = all_poi_choices(&self.area, self.point, self.heading)
                    .into_iter()
                    .take(10)
                    .map(|choice| choice.name)
                    .collect();
                self.announce(&spoken_places(&lines));
            }
            0x52 => {
                self.ensure_coverage(true);
            }
            0x42 => {
                let mut bookmarks: Vec<SearchResult> =
                    data::read("bookmarks.json").unwrap_or_default();
                bookmarks.push(SearchResult {
                    name: format!(
                        "{}: {:.5}, {:.5}",
                        self.area.name, self.point.lat, self.point.lon
                    ),
                    point: self.point,
                });
                let saved = data::save(
                    &format!("area_v9_{:.5}_{:.5}.json", self.point.lat, self.point.lon),
                    &self.area,
                )
                .and_then(|_| data::save("bookmarks.json", &bookmarks));
                match saved{Ok(())=>self.announce("Bookmark and its neighborhood saved for offline use. Press J to browse bookmarks."),Err(e)=>self.announce(&e)}
            }
            0x4A => {
                self.show_saved();
            }
            0x4D => {
                self.muted = !self.muted;
                if self.muted {
                    self.speech.stop();
                    self.audio.stop();
                }
                self.announce(if self.muted {
                    "Automatic speech and spatial cues off."
                } else {
                    "Automatic speech and spatial cues on."
                });
            }
            VK_F1 => self.help(),
            _ => {}
        }
    }
}
