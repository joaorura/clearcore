#![forbid(unsafe_code)]

use std::env;
use std::process::ExitCode;
use realtime_noise_service::bootstrap::ServiceBootstrap;
use realtime_noise_service::install::{install_user_service, uninstall_user_service};

fn print_usage() {
    println!("realtime-noise-service - Standalone User Daemon");
    println!("Usage:");
    println!("  realtime-noise-service --run                    Run daemon supervisor and IPC loop");
    println!("  realtime-noise-service --install-user-service   Install per-user autostart service");
    println!("  realtime-noise-service --uninstall-user-service Uninstall per-user autostart service");
    println!("  realtime-noise-service --help                   Show this help message");
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage();
        return ExitCode::from(1);
    }

    match args[1].as_str() {
        "--run" => {
            let mut bootstrap = ServiceBootstrap::default();
            if let Err(err) = bootstrap.run() {
                eprintln!("Daemon execution error: {err}");
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        "--install-user-service" => {
            if let Err(err) = install_user_service() {
                eprintln!("Failed to install user service: {err}");
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        "--uninstall-user-service" => {
            if let Err(err) = uninstall_user_service() {
                eprintln!("Failed to uninstall user service: {err}");
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        "--help" | "-h" => {
            print_usage();
            ExitCode::SUCCESS
        }
        unknown => {
            eprintln!("Unknown argument: {unknown}");
            print_usage();
            ExitCode::from(1)
        }
    }
}
