//! Where the popover goes. It sits beside the tray icon, on whichever side faces
//! the desktop, and reports where the notch must point. Pure arithmetic, so every
//! taskbar position can be tested.

use serde::Serialize;

/// Size of the visible panel, in logical pixels.
pub const PANEL_WIDTH: f64 = 396.0;
pub const PANEL_HEIGHT: f64 = 660.0;

/// Transparent space around the panel for its shadow and notch. Matches
/// `--stage-margin` in styles.css.
pub const MARGIN: f64 = 24.0;

/// How far the notch projects from the panel, and the gap from its tip to the icon.
const NOTCH: f64 = 8.0;
const ICON_GAP: f64 = 4.0;

/// Keeps the panel off the screen edge, and the notch off the panel's corners.
const SCREEN_INSET: f64 = 8.0;
const NOTCH_INSET: f64 = 28.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Bounds {
    fn center_x(self) -> f64 {
        self.x + self.width / 2.0
    }

    fn center_y(self) -> f64 {
        self.y + self.height / 2.0
    }

    fn right(self) -> f64 {
        self.x + self.width
    }

    fn bottom(self) -> f64 {
        self.y + self.height
    }
}

/// The panel edge that faces the icon.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Top,
    Bottom,
    Left,
    Right,
}

/// What the client needs to draw the notch and open from it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Anchor {
    pub side: Side,
    /// Distance along that edge from the window's start, in logical pixels.
    pub offset: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    /// The whole window, margin included, in physical pixels.
    pub window: Bounds,
    pub anchor: Anchor,
}

fn clamp_between(value: f64, low: f64, high: f64) -> f64 {
    if low > high { (low + high) / 2.0 } else { value.clamp(low, high) }
}

/// Places the window for a tray icon, both in physical pixels, on a monitor with
/// the given scale factor. The taskbar is whichever edge the icon lies past. An
/// icon inside the work area is in the overflow flyout, which opens upward.
pub fn place(icon: Bounds, work_area: Bounds, scale: f64) -> Placement {
    let margin = MARGIN * scale;
    let inset = SCREEN_INSET * scale;
    let width = (PANEL_WIDTH + MARGIN * 2.0) * scale;
    let tallest = work_area.height - (NOTCH + ICON_GAP + SCREEN_INSET) * scale + margin * 2.0;
    let height = ((PANEL_HEIGHT + MARGIN * 2.0) * scale).min(tallest);
    let reach = (NOTCH + ICON_GAP) * scale - margin;

    let side = if icon.center_y() >= work_area.bottom() {
        Side::Bottom
    } else if icon.center_y() <= work_area.y {
        Side::Top
    } else if icon.center_x() >= work_area.right() {
        Side::Right
    } else if icon.center_x() <= work_area.x {
        Side::Left
    } else {
        Side::Bottom
    };

    let vertical = matches!(side, Side::Top | Side::Bottom);
    let (x, y) = if vertical {
        let x = clamp_between(
            icon.center_x() - width / 2.0,
            work_area.x + inset - margin,
            work_area.right() - inset + margin - width,
        );
        let y = match side {
            Side::Top => icon.bottom() + reach,
            _ => icon.y - reach - height,
        };
        (x, y)
    } else {
        let y = clamp_between(
            icon.center_y() - height / 2.0,
            work_area.y + inset - margin,
            work_area.bottom() - inset + margin - height,
        );
        let x = match side {
            Side::Left => icon.right() + reach,
            _ => icon.x - reach - width,
        };
        (x, y)
    };

    let (along, length) = if vertical {
        (icon.center_x() - x, width)
    } else {
        (icon.center_y() - y, height)
    };
    let edge = (MARGIN + NOTCH_INSET) * scale;
    let offset = clamp_between(along, edge, length - edge) / scale;

    Placement { window: Bounds { x, y, width, height }, anchor: Anchor { side, offset } }
}
