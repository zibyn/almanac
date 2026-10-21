mod alias;
mod cli;
mod error;
mod log;
mod model;
mod project;
mod purge;
mod remind;
mod source_check;
mod state;
mod tools;
mod writer;

fn main() {
    std::process::exit(cli::run(std::env::args().skip(1).collect()));
}
