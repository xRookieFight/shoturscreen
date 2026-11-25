mod capture;
mod overlay;

use std::process::ExitCode;

fn main() -> ExitCode {
    match overlay::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("shoturscreen: {e}");
            ExitCode::FAILURE
        }
    }
}
