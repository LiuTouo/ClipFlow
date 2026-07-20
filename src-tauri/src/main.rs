// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    let hidden = args.iter().any(|a| a == "--hidden");
    clipflow_lib::run(hidden)
}
