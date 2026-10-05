#![windows_subsystem = "windows"]
mod audio;
mod data;
mod ev_audio;
mod geo;
mod google;
mod location;
mod map;
mod navigation;
mod speech;
mod ui;
fn main() {
    ui::run();
}
