mod alias;
mod analyzer;
mod cli;
mod error;
mod index;
mod log;
mod mcp;
mod model;
mod project;
mod purge;
mod remind;
mod secrets;
mod similarity;
mod source_check;
mod state;
mod tools;
mod writer;

fn main() {
    std::process::exit(cli::run(std::env::args().skip(1).collect()));
}
