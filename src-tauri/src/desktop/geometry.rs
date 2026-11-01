use projectpane_core::persistence::{WindowBounds, WorkArea, clamp_window};
use tauri::{PhysicalPosition, PhysicalSize, WebviewWindow};

pub fn normal_bounds(window: &WebviewWindow) -> tauri::Result<Option<WindowBounds>> {
    if window.is_minimized()? || window.is_maximized()? {
        return Ok(None);
    }
    let position = window.outer_position()?;
    let size = window.outer_size()?;
    Ok(Some(WindowBounds {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
        maximized: false,
    }))
}

pub fn restore(window: &WebviewWindow, saved: WindowBounds) -> Result<WindowBounds, ()> {
    let monitors = window.available_monitors().map_err(|_| ())?;
    let primary = window.primary_monitor().map_err(|_| ())?;
    let primary_index = primary.as_ref().and_then(|primary| {
        monitors.iter().position(|monitor| {
            monitor.position() == primary.position() && monitor.size() == primary.size()
        })
    });
    let areas: Vec<_> = monitors
        .iter()
        .map(|monitor| {
            let area = monitor.work_area();
            let minimum =
                tauri::LogicalSize::new(800.0, 560.0).to_physical::<u32>(monitor.scale_factor());
            WorkArea {
                x: area.position.x,
                y: area.position.y,
                width: area.size.width,
                height: area.size.height,
                min_width: minimum.width,
                min_height: minimum.height,
            }
        })
        .collect();
    let bounds = clamp_window(saved, &areas, primary_index).ok_or(())?;
    // Native inner minimums must not force the outer frame beyond a small work area.
    window
        .set_min_size(None::<PhysicalSize<u32>>)
        .map_err(|_| ())?;
    apply_outer(window, bounds)?;
    // Moving across monitors can change frame insets with DPI. Measure them again.
    apply_outer(window, bounds)?;
    let minimum = clamp_window(
        WindowBounds {
            width: 1,
            height: 1,
            ..bounds
        },
        &areas,
        primary_index,
    )
    .ok_or(())?;
    let inner_minimum = inner_size(
        minimum,
        window.outer_size().map_err(|_| ())?,
        window.inner_size().map_err(|_| ())?,
    )
    .ok_or(())?;
    window.set_min_size(Some(inner_minimum)).map_err(|_| ())?;
    if bounds.maximized {
        window.maximize().map_err(|_| ())?;
    }
    Ok(bounds)
}

fn apply_outer(window: &WebviewWindow, bounds: WindowBounds) -> Result<(), ()> {
    let outer = window.outer_size().map_err(|_| ())?;
    let inner = window.inner_size().map_err(|_| ())?;
    let size = inner_size(bounds, outer, inner).ok_or(())?;
    window.set_size(size).map_err(|_| ())?;
    window
        .set_position(PhysicalPosition::new(bounds.x, bounds.y))
        .map_err(|_| ())
}

fn inner_size(
    bounds: WindowBounds,
    outer: PhysicalSize<u32>,
    inner: PhysicalSize<u32>,
) -> Option<PhysicalSize<u32>> {
    let width = bounds
        .width
        .checked_sub(outer.width.checked_sub(inner.width)?)?;
    let height = bounds
        .height
        .checked_sub(outer.height.checked_sub(inner.height)?)?;
    (width > 0 && height > 0).then_some(PhysicalSize::new(width, height))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restored_outer_size_subtracts_frame_insets() {
        let bounds = WindowBounds {
            x: 0,
            y: 0,
            width: 800,
            height: 560,
            maximized: false,
        };
        assert_eq!(
            inner_size(
                bounds,
                PhysicalSize::new(1216, 839),
                PhysicalSize::new(1200, 800)
            ),
            Some(PhysicalSize::new(784, 521))
        );
    }
    #[test]
    fn impossible_frame_dimensions_fail_without_unsigned_wrap() {
        let bounds = WindowBounds {
            x: 0,
            y: 0,
            width: 8,
            height: 8,
            maximized: false,
        };
        assert_eq!(
            inner_size(
                bounds,
                PhysicalSize::new(1216, 839),
                PhysicalSize::new(1200, 800)
            ),
            None
        );
    }
}
