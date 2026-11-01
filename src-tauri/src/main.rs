// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Some(code) = gond_code_lib::subcommand() {
        std::process::exit(code);
    }
    gond_code_lib::run()
}
