mod alias;
mod analyzer;
mod cli;
mod error;
mod index;
mod log;
mod model;
mod project;
mod remind;
mod secrets;
mod source_check;
mod state;
mod tools;
mod writer;

fn main() {
    std::process::exit(cli::run(std::env::args().skip(1).collect()));
}
