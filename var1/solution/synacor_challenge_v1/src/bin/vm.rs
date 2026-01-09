use log::{error, warn};
use synacor_challenge_v1::config::*;
use synacor_challenge_v1::*;

fn main() {
    println!("Starting SYNACOR VM");
    env_logger::init();
    // load configuration
    let conf: Configuration = match parse_args() {
        Ok(c) => c,
        Err(e) => {
            let mut c = Configuration::default();
            error!("Failed to parse configuration. Error: {}", e);
            warn!(
                "Failed to parse configuration. Fallback to default value {:?}",
                c
            );
            if let Err(read_error) = c.read_in() {
                error!(
                    "Failed to load the default configuration. Aborting execution. Error: {}",
                    read_error
                );
                std::process::exit(2);
            }
            c
        }
    };
    // launch VM
    match run(conf) {
        Ok(()) => println!("Challenge program finished successfully"),
        Err(e) => eprintln!("Error: {}", e),
    };
}
