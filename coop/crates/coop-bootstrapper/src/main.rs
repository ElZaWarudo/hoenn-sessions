mod config;
mod launch;

fn main() {
    match launch::run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("Hoenn Sessions could not start: {error}");
            std::process::exit(1);
        }
    }
}
