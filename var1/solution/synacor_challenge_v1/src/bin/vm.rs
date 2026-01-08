use synacor_challenge_v1::*;

fn main() {
    println!("Starting SYNACOR VM");
    let conf = config::Configuration::default();
    match run(conf) {
        Ok(()) => println!("VM finished successfully"),
        Err(e) => eprintln!("Error: {}", e),
    }
}
