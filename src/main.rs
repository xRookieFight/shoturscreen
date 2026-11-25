mod capture;
mod overlay;

use std::process::ExitCode;

fn main() -> ExitCode {
    match overlay::run() {
        Ok(Some(image)) => {
            println!("{}x{}", image.width, image.height);
            ExitCode::SUCCESS
        }
        Ok(None) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("shoturscreen: {e}");
            ExitCode::FAILURE
        }
    }
}
