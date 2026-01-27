// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

fn main() {
    clipped_app_lib::run()
}
