mod capture;

fn main() {
    let names: Vec<String> = std::env::args().skip(1).collect();
    match capture::capture_screens(&names) {
        Ok(images) => {
            for image in images {
                println!("{}x{} {}", image.width, image.height, image.pixels.len());
            }
        }
        Err(e) => eprintln!("{e}"),
    }
}
