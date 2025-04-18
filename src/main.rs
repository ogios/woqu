use clap::Parser;
use key::watch_for_keys;
use rustix::{path::Arg, process::geteuid};
use sfx::{init_pool, load_data, spawn_play};

mod key;
mod sfx;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    fn is_root() -> bool {
        geteuid().is_root()
    }

    fn is_in_input_group() -> bool {
        let out = std::process::Command::new("groups").output().unwrap();
        let res = out.stdout.to_string_lossy();
        res.contains("input")
    }

    if !is_root() && !is_in_input_group() {
        eprintln!(
            "WARN: This program requires root privileges or membership in the 'input' group."
        );
    }

    let cli = Cli::parse();
    load_data(&cli.file, cli.vol_gain);
    init_pool(cli.threads);
    watch_for_keys(spawn_play).await.unwrap();
}

#[derive(Debug, Parser)]
pub struct Cli {
    #[arg(short = 'f', long)]
    pub file: String,
    #[arg(short = 'v', long)]
    pub vol_gain: Option<f32>,
    #[arg(short = 't', long)]
    pub threads: Option<usize>,
}
