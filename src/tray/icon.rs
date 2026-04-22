use std::io::Cursor;

use tray_icon::Icon;

const PNG_BYTES: &[u8] = include_bytes!("../../assets/usage-radar-64x64.png");

pub fn build() -> Result<Icon, String> {
    let decoder = png::Decoder::new(Cursor::new(PNG_BYTES));
    let mut reader = decoder
        .read_info()
        .map_err(|error| format!("Failed to read tray icon PNG: {error}"))?;

    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buffer)
        .map_err(|error| format!("Failed to decode tray icon PNG: {error}"))?;

    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return Err("Tray icon PNG must be RGBA with 8-bit channels.".to_string());
    }

    Icon::from_rgba(
        buffer[..info.buffer_size()].to_vec(),
        info.width,
        info.height,
    )
    .map_err(|error| format!("Failed to build tray icon: {error}"))
}
