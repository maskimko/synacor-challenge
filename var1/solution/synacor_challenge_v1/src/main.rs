use synacor_challenge_v1::*;
use synacor_challenge_v1::config::*;
use log::warn;

fn main() {
    println!("Welcome to SYNACOR challenge!");
    let conf: Configuration  = match parse_args() {
        Ok(c) => c, 
        Err(e) => {
    
    let c = Configuration::default();

warn!("Failed to parse configuration. Fallback to default value {:?}", c );
    c },
    };
    match run(conf) { 
        Ok(()) => println!("Challenge program finished successfully"),
        Err(e) => eprintln!("Error: {}", e),
};
}
