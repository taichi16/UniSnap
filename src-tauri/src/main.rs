// Prevents additional console window on Windows in release, DO NOT REMOVE!!
// Never create a second console window on Windows, including development
// builds launched outside an existing terminal. Development logs are still
// available through the terminal that runs `tauri dev`.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

fn main() {
    unisnap_windows_lib::run()
}
