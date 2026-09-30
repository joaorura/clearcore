#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc, clippy::missing_const_for_fn)]

pub mod commands;

use realtime_noise_ipc::DenoiseMode;
use std::env;
use std::process::ExitCode;

fn print_usage() {
    println!("realtime-noise-app-tauri - Desktop Control Client & System Tray");
    println!("Usage:");
    println!("  realtime-noise-app-tauri [options]");
    println!("Options:");
    println!("  --status                  Query service daemon status via IPC");
    println!("  --mode <active|bypass|mute> Set suppression operating mode");
    println!("  --restart                 Request supervisor engine restart");
    println!("  --help, -h                Show this help message");
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 {
        match args[1].as_str() {
            "--help" | "-h" => {
                print_usage();
                return ExitCode::SUCCESS;
            }
            "--status" => match commands::get_status() {
                Ok(status) => {
                    println!("{status}");
                    return ExitCode::SUCCESS;
                }
                Err(e) => {
                    eprintln!("Error querying status: {e}");
                    return ExitCode::from(1);
                }
            },
            "--mode" => {
                if args.len() < 3 {
                    eprintln!("Missing mode argument. Expected: active, bypass, or mute");
                    return ExitCode::from(1);
                }
                let mode = match args[2].to_lowercase().as_str() {
                    "active" => DenoiseMode::Active,
                    "bypass" => DenoiseMode::Bypass,
                    "mute" => DenoiseMode::Mute,
                    unknown => {
                        eprintln!("Unknown mode: {unknown}. Expected active, bypass, or mute");
                        return ExitCode::from(1);
                    }
                };
                match commands::set_mode(mode) {
                    Ok(resp) => {
                        println!("Mode updated successfully: {resp}");
                        return ExitCode::SUCCESS;
                    }
                    Err(e) => {
                        eprintln!("Error setting mode: {e}");
                        return ExitCode::from(1);
                    }
                }
            }
            "--restart" => match commands::restart_generation() {
                Ok(resp) => {
                    println!("Restart generation response: {resp}");
                    return ExitCode::SUCCESS;
                }
                Err(e) => {
                    eprintln!("Error restarting engine: {e}");
                    return ExitCode::from(1);
                }
            },
            unknown => {
                eprintln!("Unknown option: {unknown}");
                print_usage();
                return ExitCode::from(1);
            }
        }
    }

    println!("Starting Realtime Noise Desktop Companion...");
    ExitCode::SUCCESS
}
