mod app;
mod cli;
mod completions;
mod repl;

fn main() {
    if let Err(error) = app::run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}
