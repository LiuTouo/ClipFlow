#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::env;

fn main() {
    // The autostart shortcut passes --hidden (see CONTEXT.md). The flag is
    // accepted for spec/forward compatibility but is currently a no-op:
    // ClipFlow never opens a window at startup — the Panel only appears on
    // the global hotkey.
    let args: Vec<String> = env::args().collect();
    let hidden = args.iter().any(|a| a == "--hidden");
    clipflow_lib::run(hidden)
}
