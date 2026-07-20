#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::env;
use std::fs::OpenOptions;
use std::io::Write;

fn log(msg: &str) {
    use std::fs::OpenOptions;
    use std::io::Write;
    let log_path = std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("clipflow.log");
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&log_path) {
        let _ = writeln!(f, "{}", msg);
    }
}

fn main() {
    log("[ClipFlow] starting");
    let args: Vec<String> = env::args().collect();
    let hidden = args.iter().any(|a| a == "--hidden");
    log(&format!("[ClipFlow] hidden={}", hidden));

    if let Err(e) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        clipflow_lib::run(hidden)
    })) {
        let msg = if let Some(s) = e.downcast_ref::<String>() {
            s.clone()
        } else if let Some(s) = e.downcast_ref::<&str>() {
            s.to_string()
        } else {
            "unknown panic".to_string()
        };
        log(&format!("[ClipFlow] PANIC: {}", msg));
    }
    log("[ClipFlow] exiting");
}
