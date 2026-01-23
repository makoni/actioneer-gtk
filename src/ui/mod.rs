mod detail_placeholder;
mod detail_view;
mod ansi;
mod sidebar;
mod welcome_screen;

pub mod auth_window;
pub mod job_logs_window;
pub mod main_window;
pub mod preferences_window;
pub mod style;

// New modules for better organization
pub mod state;
pub mod tasks;
pub mod utils;

#[cfg(test)]
pub mod test_helpers;

pub use main_window::MainWindow;
pub use welcome_screen::WelcomeScreen;
