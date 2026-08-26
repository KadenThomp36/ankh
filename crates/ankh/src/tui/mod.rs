//! The full-screen interface. Modal, keyboard-driven, themed.

pub mod app;
pub mod audio;
pub mod banner;
pub mod doc;
pub mod images;
pub mod keys;
pub mod theme;
pub mod views;

use ankh_core::Paths;

pub fn run(paths: Paths) -> i32 {
    // Query the terminal for graphics support before entering the alternate screen.
    let images = images::Images::detect();
    // A panic must never leave the terminal in raw mode.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        ratatui::restore();
        default_hook(info);
    }));
    match app::App::new(paths, images) {
        Ok(app) => {
            let terminal = ratatui::init();
            let res = app.run(terminal);
            ratatui::restore();
            match res {
                Ok(()) => 0,
                Err(e) => {
                    eprintln!("error: {e:#}");
                    1
                }
            }
        }
        Err(e) => {
            eprintln!("error: {e}");
            e.exit_code()
        }
    }
}
