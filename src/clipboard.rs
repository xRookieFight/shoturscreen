use wl_clipboard_rs::copy::{MimeType, Options, Source};

use crate::capture::Image;

pub fn copy_png(image: &Image) -> Result<(), String> {
    let png = encode_png(image)?;

    let mut options = Options::new();
    options.foreground(true);
    let prepared = options
        .prepare_copy(
            Source::Bytes(png.into_boxed_slice()),
            MimeType::Specific("image/png".into()),
        )
        .map_err(|e| e.to_string())?;

    match unsafe { libc::fork() } {
        -1 => Err("fork failed".into()),
        0 => {
            unsafe { libc::setsid() };
            let _ = prepared.serve();
            std::process::exit(0);
        }
        _ => Ok(()),
    }
}

fn encode_png(image: &Image) -> Result<Vec<u8>, String> {
    let mut rgb = Vec::with_capacity(image.width as usize * image.height as usize * 3);
    for px in image.pixels.chunks_exact(4) {
        rgb.extend_from_slice(&[px[2], px[1], px[0]]);
    }

    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, image.width, image.height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(&rgb).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    Ok(out)
}
