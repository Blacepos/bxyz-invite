#![feature(duration_constructors, duration_constructors_lite)]
pub mod cli;
pub mod event_db;
pub mod init;
pub mod templates;

use clap::Parser;

use crate::event_db::EventDB;

const DEFAULT_DB_PATH: &str = "events.db";

#[derive(Parser, Debug)]
#[command(version, about = "blacepos.xyz webserver \"invite\" module")]
pub struct Args {
    #[arg(short='f', long="file", default_value=DEFAULT_DB_PATH)]
    pub db_file: String,
}


#[tokio::main]
async fn main() {
    let args = Args::parse();

    let Ok(data) = tokio::fs::read(args.db_file).await else {
        eprintln!("Failed to read database file");
        std::process::exit(1);
    };

    let Ok(db) = serde_cbor::from_slice::<EventDB>(&data) else {
        eprintln!("Failed to parse database file");
        std::process::exit(1);
    };

    println!("{db:?}");
}