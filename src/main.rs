mod app;
mod cli;
mod clipboard;
mod config;
mod error;
mod net;
mod sanitize;
mod upload;

#[cfg(not(debug_assertions))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() {
    if let Err(e) = app::run() {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
