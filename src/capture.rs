use std::collections::HashMap;
use std::io::{self, Read};
use std::os::fd::AsFd;
use std::thread;

use zbus::blocking::Connection;
use zbus::zvariant::{Fd, OwnedValue, Value};

const FORMAT_RGB32: u32 = 4;
const FORMAT_ARGB32: u32 = 5;
const FORMAT_ARGB32_PREMULTIPLIED: u32 = 6;
const FORMAT_RGBX8888: u32 = 16;
const FORMAT_RGBA8888: u32 = 17;
const FORMAT_RGBA8888_PREMULTIPLIED: u32 = 18;

pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

pub fn capture_screens(names: &[String]) -> Result<Vec<Image>, String> {
    let connection = Connection::session().map_err(|e| e.to_string())?;
    thread::scope(|scope| {
        let handles: Vec<_> = names
            .iter()
            .map(|name| {
                let connection = &connection;
                scope.spawn(move || capture_screen(connection, name))
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().map_err(|_| "capture thread panicked".to_string())?)
            .collect()
    })
}

fn capture_screen(connection: &Connection, name: &str) -> Result<Image, String> {
    let (mut reader, writer) = io::pipe().map_err(|e| e.to_string())?;
    let reader_thread = thread::spawn(move || {
        let mut data = Vec::new();
        reader.read_to_end(&mut data).map(|_| data)
    });

    let mut options: HashMap<&str, Value> = HashMap::new();
    options.insert("native-resolution", Value::Bool(true));
    options.insert("include-cursor", Value::Bool(false));
    options.insert("include-decoration", Value::Bool(false));

    let reply = connection.call_method(
        Some("org.kde.KWin"),
        "/org/kde/KWin/ScreenShot2",
        Some("org.kde.KWin.ScreenShot2"),
        "CaptureScreen",
        &(name, options, Fd::from(writer.as_fd())),
    );
    drop(writer);
    let reply = reply.map_err(|e| e.to_string())?;
    let results: HashMap<String, OwnedValue> =
        reply.body().deserialize().map_err(|e| e.to_string())?;

    let data = reader_thread
        .join()
        .map_err(|_| "pipe reader panicked".to_string())?
        .map_err(|e| e.to_string())?;

    let width = read_u32(&results, "width")?;
    let height = read_u32(&results, "height")?;
    let stride = read_u32(&results, "stride")? as usize;
    let format = read_u32(&results, "format")?;

    let row_bytes = width as usize * 4;
    if stride < row_bytes || data.len() < stride * height as usize {
        return Err(format!("short screenshot data for {name}"));
    }

    let mut pixels = Vec::with_capacity(row_bytes * height as usize);
    for row in data.chunks_exact(stride).take(height as usize) {
        pixels.extend_from_slice(&row[..row_bytes]);
    }

    match format {
        FORMAT_RGB32 | FORMAT_ARGB32 | FORMAT_ARGB32_PREMULTIPLIED => {}
        FORMAT_RGBX8888 | FORMAT_RGBA8888 | FORMAT_RGBA8888_PREMULTIPLIED => {
            for px in pixels.chunks_exact_mut(4) {
                px.swap(0, 2);
            }
        }
        other => return Err(format!("unsupported pixel format {other}")),
    }
    for px in pixels.chunks_exact_mut(4) {
        px[3] = 0xff;
    }

    Ok(Image { width, height, pixels })
}

fn read_u32(results: &HashMap<String, OwnedValue>, key: &str) -> Result<u32, String> {
    results
        .get(key)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| format!("missing {key} in screenshot reply"))
}
