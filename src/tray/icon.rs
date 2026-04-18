use tray_icon::Icon;

const ICON_SIZE: u32 = 32;

pub fn build() -> Result<Icon, String> {
    Icon::from_rgba(rgba(), ICON_SIZE, ICON_SIZE)
        .map_err(|error| format!("Failed to build tray icon: {error}"))
}

fn rgba() -> Vec<u8> {
    let mut pixels = vec![0; (ICON_SIZE * ICON_SIZE * 4) as usize];
    let center = ICON_SIZE as f32 / 2.0;

    for y in 0..ICON_SIZE {
        for x in 0..ICON_SIZE {
            let dx = x as f32 + 0.5 - center;
            let dy = y as f32 + 0.5 - center;
            let distance = (dx * dx + dy * dy).sqrt();
            let index = ((y * ICON_SIZE + x) * 4) as usize;

            let mut rgba = [10, 15, 23, 0];

            if distance <= 14.0 {
                rgba = [11, 17, 29, 255];
            }

            if (distance - 11.0).abs() <= 1.3 && dy <= 0.0 {
                rgba = [76, 192, 142, 255];
            }

            if distance <= 3.0 {
                rgba = [76, 192, 142, 255];
            }

            if x >= 16 && x <= 27 && y >= 16 && y <= 18 {
                rgba = [76, 192, 142, 255];
            }

            pixels[index..index + 4].copy_from_slice(&rgba);
        }
    }

    pixels
}
