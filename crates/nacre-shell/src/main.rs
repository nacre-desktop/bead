// SPDX-License-Identifier: GPL-3.0-only
use bead_wayland::{Options, run};

fn main() {
    if let Err(error) = start() {
        eprintln!("nacre-shell: {error}");
        std::process::exit(1);
    }
}

fn start() -> Result<(), Box<dyn std::error::Error>> {
    let mut options = Options::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--namespace" => options.namespace = args.next().ok_or("--namespace needs a value")?,
            "--help" | "-h" => {
                println!(
                    "nacre-shell [--namespace NAME]\nFeature 1: blank, click-through strips on every Wayland output.\nDefault namespace: bead-dev. Stop with Ctrl+C or SIGTERM."
                );
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {arg}").into()),
        }
    }
    run(options)
}
