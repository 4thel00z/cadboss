#![deny(unsafe_code)]

use clap::Parser;

#[derive(Parser)]
#[command(
    name = "cadboss",
    version,
    about = "CAD parsing, text extraction and rendering"
)]
struct Cli {}

fn main() {
    Cli::parse();
}
