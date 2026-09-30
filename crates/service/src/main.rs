#![forbid(unsafe_code)]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("realtime-noise-service - Standalone User Daemon");
        println!("Usage: realtime-noise-service [--run | --install-user-service | --uninstall-user-service]");
    }
}
