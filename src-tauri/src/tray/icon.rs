//! Draws the tray icon. The design never changes: a rounded tile with the stat
//! in white and, for percentages, a level rising from the bottom edge. The tile
//! carries its own background, so it reads on light and dark taskbars alike.

use crate::sources::Unit;

pub const SIZE: u32 = 32;

const RADIUS: f32 = 7.0;
const BASE: [u8; 3] = [0x1F, 0x20, 0x25];
const LEVEL: [u8; 3] = [0x3F, 0x6E, 0x94];
const TEXT: [u8; 3] = [0xF4, 0xEF, 0xE6];

/// The stat as it appears on the icon: three characters at most.
pub fn short_text(unit: Unit, value: f64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    const TB: f64 = GB * 1024.0;

    let scaled = |value: f64, divisor: f64, suffix: char| {
        format!("{}{suffix}", ((value / divisor).round() as u64).min(99))
    };

    match unit {
        Unit::Percent => format!("{}", value.round().clamp(0.0, 100.0) as u32),
        Unit::Celsius => format!("{}", value.round().clamp(0.0, 999.0) as u32),
        Unit::Count | Unit::Amount => match value {
            v if v >= 1e9 => scaled(v, 1e9, 'G'),
            v if v >= 1e6 => scaled(v, 1e6, 'M'),
            v if v >= 1e3 => scaled(v, 1e3, 'K'),
            v => format!("{}", v.round() as u32),
        },
        Unit::Timestamp => "--".to_owned(),
        Unit::Bytes | Unit::BytesPerSecond => match value {
            v if v >= TB => scaled(v, TB, 'T'),
            v if v >= GB => scaled(v, GB, 'G'),
            v if v >= MB => scaled(v, MB, 'M'),
            v => scaled(v, KB, 'K'),
        },
        Unit::Seconds => match value / 3600.0 {
            hours if hours >= 48.0 => scaled(hours, 24.0, 'D'),
            hours => scaled(hours, 1.0, 'H'),
        },
    }
}

/// The stat as it appears in the tooltip.
pub fn long_text(unit: Unit, value: f64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];

    let bytes = |value: f64| {
        let mut scaled = value;
        let mut index = 0;
        while scaled >= 1024.0 && index < UNITS.len() - 1 {
            scaled /= 1024.0;
            index += 1;
        }
        format!("{scaled:.1} {}", UNITS[index])
    };

    match unit {
        Unit::Percent => format!("{value:.0}%"),
        Unit::Celsius => format!("{value:.0}\u{b0}C"),
        Unit::Count | Unit::Amount => format!("{value:.0}"),
        Unit::Timestamp => "--".to_owned(),
        Unit::Bytes => bytes(value),
        Unit::BytesPerSecond => format!("{}/s", bytes(value)),
        Unit::Seconds => {
            let hours = (value / 3600.0) as u64;
            format!("{}d {}h", hours / 24, hours % 24)
        }
    }
}

/// Fraction of the tile to fill, for units that have a natural maximum.
pub fn level(unit: Unit, value: f64) -> Option<f32> {
    matches!(unit, Unit::Percent).then(|| (value / 100.0).clamp(0.0, 1.0) as f32)
}

/// 3 by 5 glyphs, one row per entry, low three bits set where a pixel is drawn.
fn glyph(c: char) -> [u8; 5] {
    match c {
        '0' => [0b111, 0b101, 0b101, 0b101, 0b111],
        '1' => [0b010, 0b110, 0b010, 0b010, 0b111],
        '2' => [0b111, 0b001, 0b111, 0b100, 0b111],
        '3' => [0b111, 0b001, 0b011, 0b001, 0b111],
        '4' => [0b101, 0b101, 0b111, 0b001, 0b001],
        '5' => [0b111, 0b100, 0b111, 0b001, 0b111],
        '6' => [0b111, 0b100, 0b111, 0b101, 0b111],
        '7' => [0b111, 0b001, 0b010, 0b010, 0b010],
        '8' => [0b111, 0b101, 0b111, 0b101, 0b111],
        '9' => [0b111, 0b101, 0b111, 0b001, 0b111],
        'K' => [0b101, 0b101, 0b110, 0b101, 0b101],
        'M' => [0b101, 0b111, 0b111, 0b101, 0b101],
        'G' => [0b111, 0b100, 0b101, 0b101, 0b111],
        'T' => [0b111, 0b010, 0b010, 0b010, 0b010],
        'D' => [0b110, 0b101, 0b101, 0b101, 0b110],
        'H' => [0b101, 0b101, 0b111, 0b101, 0b101],
        '-' => [0b000, 0b000, 0b111, 0b000, 0b000],
        _ => [0; 5],
    }
}

/// Renders straight-alpha RGBA, `SIZE` by `SIZE`.
pub fn render(text: &str, level: Option<f32>) -> Vec<u8> {
    let size = SIZE as usize;
    let mut pixels = vec![0u8; size * size * 4];
    let level_top = level.map(|level| SIZE as f32 * (1.0 - level));

    for y in 0..size {
        for x in 0..size {
            let color = match level_top {
                Some(top) if (y as f32) < top => BASE,
                Some(_) => LEVEL,
                None => BASE,
            };
            let offset = (y * size + x) * 4;
            pixels[offset..offset + 3].copy_from_slice(&color);
            pixels[offset + 3] = (coverage(x, y) * 255.0) as u8;
        }
    }

    draw_text(&mut pixels, text);
    pixels
}

/// How much of a pixel lies inside the rounded tile, from its signed distance.
fn coverage(x: usize, y: usize) -> f32 {
    let half = SIZE as f32 / 2.0;
    let dx = (x as f32 + 0.5 - half).abs() - (half - RADIUS);
    let dy = (y as f32 + 0.5 - half).abs() - (half - RADIUS);
    let outside = dx.max(0.0).hypot(dy.max(0.0)) + dx.max(dy).min(0.0) - RADIUS;
    (0.5 - outside).clamp(0.0, 1.0)
}

fn draw_text(pixels: &mut [u8], text: &str) {
    let chars: Vec<char> = text.chars().collect();
    let count = chars.len();
    if count == 0 {
        return;
    }

    // Two characters can be drawn larger than three.
    let (scale, gap) = if count <= 2 { (4, 3) } else { (3, 2) };
    let width = count * 3 * scale + (count - 1) * gap;
    let height = 5 * scale;
    let size = SIZE as usize;
    let left = size.saturating_sub(width) / 2;
    let top = (size - height) / 2;

    for (index, &c) in chars.iter().enumerate() {
        let origin = left + index * (3 * scale + gap);
        for (row, bits) in glyph(c).into_iter().enumerate() {
            for column in 0..3 {
                if bits & (0b100 >> column) == 0 {
                    continue;
                }
                for y in top + row * scale..top + (row + 1) * scale {
                    for x in origin + column * scale..origin + (column + 1) * scale {
                        if x < size && y < size {
                            let offset = (y * size + x) * 4;
                            pixels[offset..offset + 3].copy_from_slice(&TEXT);
                        }
                    }
                }
            }
        }
    }
}
