mod capture;
mod clipboard;
mod overlay;

use std::process::ExitCode;

fn main() -> ExitCode {
    let result = overlay::run().and_then(|image| match image {
        Some(image) => clipboard::copy_png(&image),
        None => Ok(()),
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("shoturscreen: {e}");
            ExitCode::FAILURE
        }
    }
}
