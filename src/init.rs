//! Initialization logic
//!
//! Validates command line arguments and sets up logging

use crate::cli;
use clap::Parser;
use flexi_logger::LoggerHandle;

/// Parse command line arguments and setup logger
pub fn initialize() -> (cli::Args, LoggerHandle) {
    // parse command line arguments
    let args = cli::Args::parse();

    // setup logger
    let logger = flexi_logger::Logger::with(args.log_level)
        .format(flexi_logger::colored_opt_format);

    let logger_handle = match logger.start() {
        Ok(logger_handle) => logger_handle,
        Err(e) => {
            eprintln!("Fatal: Unable to start logger: \"{e}\"");
            std::process::exit(1);
        }
    };

    // logger handle must not be dropped per docs
    (args, logger_handle)
}
