//! The desktop app's entry point.
//!
//! Everything of substance is in the library, so that it can be tested without
//! a window; this file only starts it. The subsystem attribute hides the console
//! window a release build would otherwise carry around on Windows.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    wnd_gui::run();
}
