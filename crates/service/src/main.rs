#![forbid(unsafe_code)]

use realtime_noise_model::ProfileStore;
use realtime_noise_service::bootstrap::{ServiceBootstrap, ServiceConfig};
use realtime_noise_service::install::{install_user_service, uninstall_user_service};
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

fn print_usage() {
    println!("realtime-noise-service - Standalone User Daemon");
    println!("Usage:");
    println!(
        "  realtime-noise-service --run [options]          Run daemon supervisor and IPC loop"
    );
    println!("    Options:");
    println!(
        "      --backend <name>       Initial inference backend (auto, openvino-npu, tract, etc.)"
    );
    println!("      --models-dir <path>    Explicit path to models/stateful directory");
    println!(
        "  realtime-noise-service --install-user-service   Install per-user autostart service"
    );
    println!(
        "  realtime-noise-service --uninstall-user-service Uninstall per-user autostart service"
    );
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
            let mut initial_backend = None;
            let mut model_dir = None;
            let mut i = 2;
            while i < args.len() {
                if args[i] == "--backend" && i + 1 < args.len() {
                    initial_backend = Some(args[i + 1].clone());
                    i += 2;
                } else if args[i] == "--models-dir" && i + 1 < args.len() {
                    model_dir = Some(PathBuf::from(args[i + 1].clone()));
                    i += 2;
                } else {
                    i += 1;
                }
            }

            if let Err(e) = realtime_noise_service::logger::DaemonLogger::init() {
                eprintln!("Warning: failed to initialize daemon file logger: {e}");
            }
            realtime_noise_service::log_info!(
                "DAEMON",
                "realtime-noise-service starting (PID: {}, args: {:?})",
                std::process::id(),
                env::args().collect::<Vec<_>>()
            );

            let config = ServiceConfig {
                initial_backend,
                model_dir,
                ..Default::default()
            };
            let mut bootstrap = ServiceBootstrap::new(config);
            match ProfileStore::default_dir() {
                Some(dir) => {
                    realtime_noise_service::log_info!(
                        "DAEMON",
                        "Attaching profile store at {}",
                        dir.display()
                    );
                    bootstrap
                        .daemon_mut()
                        .attach_profile_store(ProfileStore::new(dir));
                }
                None => {
                    realtime_noise_service::log_warn!(
                        "DAEMON",
                        "No data directory available: voice profiles cannot be stored or restored"
                    );
                    eprintln!(
                        "No data directory available: voice profiles cannot be stored or restored"
                    );
                }
            }
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
