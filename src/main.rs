mod app;
mod cli;
mod clipboard;
mod config;
mod error;
mod net;
mod sanitize;
mod upload;

fn main() {
    if let Err(e) = app::run() {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
