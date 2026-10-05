use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum View {
    Explore,
    Search,
    Nearby,
    Route,
    Saved,
    Preview,
    Settings,
}
#[derive(Clone, Copy)]
pub(super) struct Positioned {
    pub(super) handle: HWND,
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) width: i32,
    pub(super) height: i32,
}
impl Positioned {
    pub(super) fn new(handle: HWND, x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            handle,
            x,
            y,
            width,
            height,
        }
    }
}
#[derive(PartialEq, Eq)]
pub(super) enum BrowseMode {
    Search,
    Categories,
    Places,
    AllPlaces,
    Explore,
    Preview,
    Settings,
}
pub(super) fn page_bounds(total: usize, requested: usize) -> (usize, usize, usize, usize) {
    let pages = total.div_ceil(PAGE_SIZE).max(1);
    let page = requested.min(pages - 1);
    let start = page * PAGE_SIZE;
    (page, start, (start + PAGE_SIZE).min(total), pages)
}
pub(super) fn page_selection_index(page: usize, local: isize, total: usize) -> Option<usize> {
    if local < 0 || local as usize >= PAGE_SIZE {
        return None;
    }
    let index = page * PAGE_SIZE + local as usize;
    (index < total).then_some(index)
}
pub(super) fn spoken_places(lines: &[String]) -> String {
    if lines.is_empty() {
        "No nearby places.".into()
    } else {
        format!("{}.", lines.join(". "))
    }
}
pub(super) fn all_poi_choices(area: &Area, position: Point, heading: f64) -> Vec<SearchResult> {
    area.nearby(position)
        .into_iter()
        .filter(|&i| area.places[i].kind != "address")
        .map(|i| SearchResult {
            name: area.describe_place(i, position, heading),
            point: area.places[i].point,
        })
        .collect()
}
impl App {
    pub(super) fn show_preview(&mut self) {
        if self.driving_settings.google_mode {
            self.announce("Street Preview needs road geometry, which Google Maps does not provide to this mode. Free walking, places, addresses, and routes remain available.");
            return;
        }
        self.preview_options = self.area.preview_roads(self.point);
        if self.preview_options.is_empty() {
            self.show_view(View::Explore);
            unsafe { SetFocus(self.walk) };
            self.announce("No road decision point within 120 metres. Move closer to a mapped street or press S to snap to one.");
            return;
        }
        self.browse_mode = BrowseMode::Preview;
        self.choices = self
            .preview_options
            .iter()
            .map(|road| SearchResult {
                name: format!(
                    "{} toward {}. {:.0} metres to the next junction or road end.",
                    road.name,
                    crate::geo::compass(road.bearing),
                    road.distance
                ),
                point: road.destination,
            })
            .collect();
        self.page = 0;
        self.show_view(View::Preview);
        self.render_results_page();
        unsafe { SetFocus(self.results) };
        let gap = self.point.distance(self.preview_options[0].origin);
        self.announce(&format!("Street preview: {} road choices at the nearest decision point, {:.0} metres away. Up and Down browse; Page Up and Down change pages; Enter moves to the next decision point; Escape returns to free walking.", self.preview_options.len(), gap));
    }
    pub(super) fn show_saved(&mut self) {
        self.destination_mode = false;
        self.browse_mode = BrowseMode::Search;
        self.choices = data::read("bookmarks.json").unwrap_or_default();
        self.page = 0;
        self.show_view(View::Saved);
        self.render_results_page();
        unsafe { SetFocus(self.results) };
        self.announce(&format!("{} saved places. Up and Down browse across pages; Enter jumps; Ctrl+Enter routes. B saves your current position.", self.choices.len()));
    }
    pub(super) fn show_route(&mut self) {
        if self.route.is_none() {
            self.open_search(true);
            self.announce("Search for a route destination.");
            return;
        }
        self.route_page = 0;
        self.show_view(View::Route);
        self.render_route_page();
        unsafe { SetFocus(self.route_list) };
    }
    pub(super) fn open_search(&mut self, destination: bool) {
        self.destination_mode = destination;
        self.browse_mode = BrowseMode::Search;
        self.choices.clear();
        self.page = 0;
        self.show_view(View::Search);
        self.render_results_page();
        unsafe {
            SetFocus(self.search);
            SendMessageW(self.search, EM_SETSEL, 0, -1);
        }
    }
    pub(super) fn selected_choice_index(&self) -> Option<usize> {
        let local = unsafe { SendMessageW(self.results, LB_GETCURSEL, 0, 0) };
        let total = match self.browse_mode {
            BrowseMode::Categories => self.categories.len(),
            BrowseMode::Settings => self.driving_settings.labels().len(),
            _ => self.choices.len(),
        };
        page_selection_index(self.page, local, total)
    }
    pub(super) fn render_results_page(&mut self) {
        let settings_labels = self.driving_settings.labels();
        let total = match self.browse_mode {
            BrowseMode::Categories => self.categories.len(),
            BrowseMode::Settings => settings_labels.len(),
            _ => self.choices.len(),
        };
        let (page, start, end, pages) = page_bounds(total, self.page);
        self.page = page;
        let title = match self.browse_mode {
            BrowseMode::Categories => "Categories",
            BrowseMode::Places => "Nearby places",
            BrowseMode::AllPlaces => "All POIs",
            BrowseMode::Explore => "Directional places",
            BrowseMode::Preview => "Road choices",
            BrowseMode::Settings => "Map and driving options",
            BrowseMode::Search if self.view == View::Saved => "Saved places",
            BrowseMode::Search => "Search results",
        };
        unsafe {
            text(
                self.results_controls[0].handle,
                &format!(
                    "{title}: {}–{} of {total}, page {} of {pages}",
                    if total == 0 { 0 } else { start + 1 },
                    end,
                    self.page + 1
                ),
            );
            SendMessageW(self.results, LB_RESETCONTENT, 0, 0);
            #[expect(
                clippy::needless_range_loop,
                reason = "the index selects from one of three lists depending on browse mode"
            )]
            for i in start..end {
                let label = if self.browse_mode == BrowseMode::Settings {
                    settings_labels[i].clone()
                } else if self.browse_mode == BrowseMode::Categories {
                    let category = &self.categories[i];
                    let count = self
                        .area
                        .places
                        .iter()
                        .filter(|p| category == "All places" || p.category() == category)
                        .count();
                    format!("{category} ({count} places)")
                } else {
                    self.choices[i].name.clone()
                };
                SendMessageW(
                    self.results,
                    LB_ADDSTRING,
                    0,
                    wide(&label).as_ptr() as isize,
                );
            }
            if end > start {
                SendMessageW(self.results, LB_SETCURSEL, 0, 0);
            }
            EnableWindow(self.page_previous, (self.page > 0) as i32);
            EnableWindow(self.page_next, (self.page + 1 < pages) as i32);
        }
    }
    pub(super) fn render_route_page(&mut self) {
        let Some(guidance) = &self.route else { return };
        let total = guidance.route.maneuvers.len();
        let (page, start, end, pages) = page_bounds(total, self.route_page);
        self.route_page = page;
        unsafe {
            text(
                self.route_title,
                &format!(
                    "Route to {}: {:.0} metres, {} instructions; page {} of {}",
                    guidance.route.name,
                    guidance.route.length(),
                    total,
                    self.route_page + 1,
                    pages
                ),
            );
            SendMessageW(self.route_list, LB_RESETCONTENT, 0, 0);
            for i in start..end {
                let label = format!("{}. {}", i + 1, guidance.route.maneuvers[i].text);
                SendMessageW(
                    self.route_list,
                    LB_ADDSTRING,
                    0,
                    wide(&label).as_ptr() as isize,
                );
            }
            if end > start {
                SendMessageW(self.route_list, LB_SETCURSEL, 0, 0);
            }
            EnableWindow(self.page_previous, (self.route_page > 0) as i32);
            EnableWindow(self.page_next, (self.route_page + 1 < pages) as i32);
        }
    }
    pub(super) fn change_page(&mut self, delta: isize, last_item: bool) {
        let route = self.view == View::Route;
        let total = if route {
            self.route.as_ref().map_or(0, |g| g.route.maneuvers.len())
        } else if self.browse_mode == BrowseMode::Settings {
            self.driving_settings.labels().len()
        } else if self.browse_mode == BrowseMode::Categories {
            self.categories.len()
        } else {
            self.choices.len()
        };
        let current = if route { self.route_page } else { self.page };
        let pages = page_bounds(total, current).3;
        let next = current.saturating_add_signed(delta).min(pages - 1);
        if next == current {
            return;
        }
        if route {
            self.route_page = next;
            self.render_route_page();
        } else {
            self.page = next;
            self.render_results_page();
        }
        let list = if route { self.route_list } else { self.results };
        unsafe {
            if last_item {
                let count = SendMessageW(list, LB_GETCOUNT, 0, 0);
                if count > 0 {
                    SendMessageW(list, LB_SETCURSEL, (count - 1) as usize, 0);
                }
            }
            SetFocus(list);
        }
        self.announce(&format!(
            "Page {} of {}. {} to {} of {}.",
            next + 1,
            pages,
            next * PAGE_SIZE + 1,
            ((next + 1) * PAGE_SIZE).min(total),
            total
        ));
    }
    pub(super) fn show_view(&mut self, view: View) {
        self.view = view;
        unsafe {
            text(
                self.view_title,
                match view {
                    View::Explore => "Explore",
                    View::Search => "Search",
                    View::Nearby => "Nearby places",
                    View::Route => "Active route",
                    View::Saved => "Saved places",
                    View::Preview => "Street preview",
                    View::Settings => "Map and driving options",
                },
            );
            for control in &self.search_controls {
                ShowWindow(
                    control.handle,
                    if view == View::Search {
                        SW_SHOW
                    } else {
                        SW_HIDE
                    },
                );
            }
            for control in &self.results_controls {
                ShowWindow(
                    control.handle,
                    if (view != View::Settings
                        || control.handle == self.results
                        || control.handle == self.results_controls[0].handle)
                        && matches!(
                            view,
                            View::Search
                                | View::Nearby
                                | View::Saved
                                | View::Preview
                                | View::Settings
                        )
                    {
                        SW_SHOW
                    } else {
                        SW_HIDE
                    },
                );
                let offset = if matches!(
                    view,
                    View::Nearby | View::Saved | View::Preview | View::Settings
                ) {
                    -58
                } else {
                    0
                };
                MoveWindow(
                    control.handle,
                    control.x,
                    control.y + offset,
                    control.width,
                    control.height,
                    1,
                );
            }
            let page_y = if view == View::Search { 144 } else { 86 };
            for (handle, x, width) in [(self.page_previous, 540, 110), (self.page_next, 660, 115)] {
                MoveWindow(handle, x, page_y, width, 24, 1);
                ShowWindow(
                    handle,
                    if matches!(view, View::Explore | View::Settings) {
                        SW_HIDE
                    } else {
                        SW_SHOW
                    },
                );
            }
            for control in &self.route_controls {
                ShowWindow(
                    control.handle,
                    if view == View::Route {
                        SW_SHOW
                    } else {
                        SW_HIDE
                    },
                );
            }
            for control in &self.settings_controls {
                ShowWindow(
                    control.handle,
                    if view == View::Settings {
                        SW_SHOW
                    } else {
                        SW_HIDE
                    },
                );
            }
            let offset = match view {
                View::Explore => -176,
                View::Nearby | View::Saved | View::Preview | View::Settings => -55,
                View::Search | View::Route => 0,
            };
            for control in &self.walking_controls {
                let height = if control.handle == self.output {
                    match view {
                        View::Explore => 364,
                        View::Nearby | View::Saved | View::Preview | View::Settings => 243,
                        View::Search | View::Route => 188,
                    }
                } else {
                    control.height
                };
                MoveWindow(
                    control.handle,
                    control.x,
                    control.y + offset,
                    control.width,
                    height,
                    1,
                );
            }
        }
    }
    pub(super) fn explore(&mut self, ahead: bool) {
        self.show_view(View::Nearby);
        let indices = self
            .area
            .exploration_places(self.point, self.heading, ahead);
        self.choices = indices
            .iter()
            .map(|&i| SearchResult {
                name: self.area.describe_place(i, self.point, self.heading),
                point: self.area.places[i].point,
            })
            .collect();
        self.browse_mode = BrowseMode::Explore;
        self.page = 0;
        self.render_results_page();
        if !self.choices.is_empty() {
            unsafe { SetFocus(self.results) };
        }
        let mode = if ahead { "Ahead of me" } else { "Around me" };
        if self.choices.is_empty() {
            self.announce(&format!("{mode}: no named places in the downloaded map. Try Nearby for all mapped features."));
        } else {
            self.announce(&format!(
                "{mode}: {} places. Use Up and Down to explore across pages; Enter jumps; Ctrl+Enter routes. Backspace returns to walking.",
                self.choices.len()
            ));
        }
    }
    pub(super) fn show_all_places(&mut self) {
        self.browse_mode = BrowseMode::AllPlaces;
        self.choices = all_poi_choices(&self.area, self.point, self.heading);
        self.page = 0;
        self.show_view(View::Nearby);
        self.render_results_page();
        unsafe { SetFocus(self.results) };
        self.announce(&format!("All POIs: {} places, nearest first. Up and Down browse across pages; Enter jumps; Ctrl+Enter or G routes. Backspace returns to walking.", self.choices.len()));
    }
    pub(super) fn search(&mut self) {
        unsafe {
            let len = GetWindowTextLengthW(self.search);
            let mut value = vec![0u16; len as usize + 1];
            GetWindowTextW(self.search, value.as_mut_ptr(), len + 1);
            let query = String::from_utf16_lossy(&value[..len as usize]);
            if query.trim().is_empty() {
                self.announce("Enter a town, address, or latitude, longitude.");
                return;
            }
            let local_address = query.split(',').next().unwrap_or("").trim();
            if local_address
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit())
            {
                let matches: Vec<_> = self
                    .area
                    .places
                    .iter()
                    .filter(|place| {
                        place.category() == "Addresses"
                            && place.name.eq_ignore_ascii_case(local_address)
                    })
                    .map(|place| SearchResult {
                        name: format!("{} (mapped in current neighborhood)", place.name),
                        point: place.point,
                    })
                    .collect();
                if !matches.is_empty() {
                    self.fill_results(matches);
                    return;
                }
            }
            self.start(Job::Search(query, self.point));
        }
    }
    pub(super) fn load_selected(&mut self) {
        if self.browse_mode == BrowseMode::Settings {
            self.adjust_settings(1);
            return;
        }
        if self.browse_mode == BrowseMode::Categories {
            self.select_category();
            return;
        }
        if self.browse_mode == BrowseMode::Preview {
            if let Some(road) = self
                .selected_choice_index()
                .and_then(|i| self.preview_options.get(i))
                .cloned()
            {
                self.move_to(road.destination, road.bearing);
                self.show_preview();
            } else {
                self.announce("Choose a road first.");
            }
            return;
        }
        if let Some(result) = self
            .selected_choice_index()
            .and_then(|i| self.choices.get(i))
            .cloned()
        {
            if matches!(
                self.browse_mode,
                BrowseMode::Places | BrowseMode::AllPlaces | BrowseMode::Explore
            ) {
                self.move_to(result.point, self.heading);
                self.show_view(View::Explore);
                unsafe { SetFocus(self.walk) };
                self.describe();
            } else {
                self.start(Job::Load(result));
            }
        } else {
            self.announce("Choose a result first.");
        }
    }
    pub(super) fn fill_results(&mut self, choices: Vec<SearchResult>) {
        self.show_view(View::Search);
        self.browse_mode = BrowseMode::Search;
        self.choices = choices;
        self.page = 0;
        self.render_results_page();
        if !self.choices.is_empty() {
            unsafe { SetFocus(self.results) };
        }
        self.announce(&format!(
            "{} results in {} pages. {}",
            self.choices.len(),
            page_bounds(self.choices.len(), 0).3,
            if self.choices.is_empty() {
                "No matching numbered address or place found. Try a different spelling or nearby address."
            } else if self.destination_mode {
                "Enter plans a walking route from your current position. Page Up and Page Down browse more results."
            } else {
                "Enter moves here; Ctrl+Enter plans a walking route here. Use Page Up and Page Down for more results."
            }
        ));
    }
    pub(super) fn show_categories(&mut self) {
        self.show_view(View::Nearby);
        self.categories = [
            "All places",
            "Crossings",
            "Addresses",
            "Food and drink",
            "Shopping",
            "Community and services",
            "Healthcare",
            "Transit",
            "Outdoors",
            "Sights",
            "Workplaces",
            "Other places",
        ]
        .iter()
        .filter(|category| {
            *category == &"All places"
                || self.area.places.iter().any(|p| p.category() == **category)
        })
        .map(|category| (*category).to_string())
        .collect();
        self.browse_mode = BrowseMode::Categories;
        self.choices.clear();
        self.page = 0;
        self.render_results_page();
        unsafe { SetFocus(self.results) };
        self.announce(
            "Categories. Use Up and Down across pages, then Enter to open a list of places.",
        );
    }
    pub(super) fn select_category(&mut self) {
        let Some(category) = self
            .selected_choice_index()
            .and_then(|i| self.categories.get(i))
        else {
            return;
        };
        let category = category.clone();
        self.show_view(View::Nearby);
        let choices = self
            .area
            .nearby(self.point)
            .into_iter()
            .filter(|i| category == "All places" || self.area.places[*i].category() == category)
            .map(|i| {
                let place = &self.area.places[i];
                SearchResult {
                    name: self.area.describe_place(i, self.point, self.heading),
                    point: place.point,
                }
            })
            .collect::<Vec<_>>();
        self.browse_mode = BrowseMode::Places;
        self.choices = choices;
        self.page = 0;
        self.render_results_page();
        unsafe { SetFocus(self.results) };
        self.announce(&format!(
            "{}: {} places, nearest first. Up and Down browse across pages; Enter jumps; Ctrl+Enter routes.",
            category,
            self.choices.len()
        ));
        if !self.muted && unsafe { GetForegroundWindow() == self.hwnd } {
            if let Some(first) = self.choices.first() {
                self.cue_point(first.point);
            }
        }
    }
}
