//! The full-screen interface. Modal, keyboard-driven, themed.

pub mod app;
pub mod banner;
pub mod keys;
pub mod theme;
pub mod views;

use ankh_core::Paths;

pub fn run(paths: Paths) -> i32 {
    match app::App::new(paths) {
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
