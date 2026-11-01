use serde::{Deserialize, Serialize};

/// Physical outer-window pixels, independent of the current monitor DPI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WindowBounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub maximized: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct WorkArea {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub min_width: u32,
    pub min_height: u32,
}

pub fn clamp_window(
    bounds: WindowBounds,
    areas: &[WorkArea],
    primary: Option<usize>,
) -> Option<WindowBounds> {
    let mut selected = None;
    let mut largest = 0_u64;
    for (index, area) in areas.iter().enumerate() {
        let overlap = intersection(bounds, *area);
        if overlap > largest {
            selected = Some(index);
            largest = overlap;
        }
    }
    let area = selected
        .and_then(|i| areas.get(i))
        .or_else(|| primary.and_then(|i| areas.get(i)))
        .or_else(|| areas.first())?;
    if area.width == 0 || area.height == 0 {
        return None;
    }
    let width = bounds
        .width
        .clamp(area.min_width.max(1).min(area.width), area.width);
    let height = bounds
        .height
        .clamp(area.min_height.max(1).min(area.height), area.height);
    let x = i64::from(bounds.x).clamp(
        i64::from(area.x),
        i64::from(area.x) + i64::from(area.width - width),
    );
    let y = i64::from(bounds.y).clamp(
        i64::from(area.y),
        i64::from(area.y) + i64::from(area.height - height),
    );
    Some(WindowBounds {
        x: i32::try_from(x).ok()?,
        y: i32::try_from(y).ok()?,
        width,
        height,
        maximized: bounds.maximized,
    })
}

fn intersection(bounds: WindowBounds, area: WorkArea) -> u64 {
    let left = i64::from(bounds.x).max(i64::from(area.x));
    let top = i64::from(bounds.y).max(i64::from(area.y));
    let right = (i64::from(bounds.x) + i64::from(bounds.width))
        .min(i64::from(area.x) + i64::from(area.width));
    let bottom = (i64::from(bounds.y) + i64::from(bounds.height))
        .min(i64::from(area.y) + i64::from(area.height));
    u64::try_from((right - left).max(0)).unwrap_or(0)
        * u64::try_from((bottom - top).max(0)).unwrap_or(0)
}
