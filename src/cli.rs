//! Defines the command line interface
//!
//! Adding attributes to this structure will add CLI options

use clap::Parser;
use std::net::IpAddr;

const DEFAULT_LOG_LEVEL: &str = "INFO";
const DEFAULT_BIND: &str = "127.0.0.1";
const DEFAULT_HTTP_PORT: &str = "0";

#[derive(Parser, Debug)]
#[command(version, about = "blacepos.xyz webserver \"invite\" module")]
pub struct Args {
    /// Log level (ERROR, WARN, INFO, DEBUG, TRACE)
    #[arg(short='l', long="log", default_value=DEFAULT_LOG_LEVEL)]
    pub log_level: log::LevelFilter,

    /// The web server bind address e.g., "127.0.0.1"
    #[arg(short='w', long="web-interface", default_value=DEFAULT_BIND)]
    pub web_addr: IpAddr,

    /// The web server HTTP bind port e.g., "80". Best to leave 0 if using Slot
    #[arg(short='H', long="http-bind", default_value=DEFAULT_HTTP_PORT)]
    pub http_port: u16,

    /// The port of the Slot server on localhost e.g., "7568"
    #[arg(short = 's', long = "slot-addr")]
    pub slot_port: Option<u16>,
}
