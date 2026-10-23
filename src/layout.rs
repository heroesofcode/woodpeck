//! Responsive geometry in logical pixels; all dimensions are bounded by the viewport.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutMode {
    Stacked,
    Columns,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Navigation {
    TopBar,
    IconRail,
    Sidebar,
}
#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub mode: LayoutMode,
    pub navigation: Navigation,
    pub board_size: f32,
    pub panel_width: f32,
    pub navigation_width: f32,
}
pub fn calculate(width: f32, height: f32) -> Layout {
    let width = if width.is_finite() {
        width.max(0.0)
    } else {
        0.0
    };
    let height = if height.is_finite() {
        height.max(0.0)
    } else {
        0.0
    };
    let navigation = if width < 640.0 {
        Navigation::TopBar
    } else if width < 1200.0 {
        Navigation::IconRail
    } else {
        Navigation::Sidebar
    };
    let navigation_width = match navigation {
        Navigation::TopBar => 0.0,
        Navigation::IconRail => 56.0,
        Navigation::Sidebar => 216.0,
    };
    let available = (width - navigation_width - 32.0).max(0.0);
    let mode = if width < 900.0 {
        LayoutMode::Stacked
    } else {
        LayoutMode::Columns
    };
    let panel_width = if mode == LayoutMode::Columns {
        if width >= 1200.0 { 320.0 } else { 288.0 }
    } else {
        available
    };
    let board_width = if mode == LayoutMode::Columns {
        (available - panel_width - 24.0).max(0.0)
    } else {
        available
    };
    Layout {
        mode,
        navigation,
        board_size: board_width.min((height - 96.0).max(0.0)),
        panel_width,
        navigation_width,
    }
}
/// Geometry within the content area after the navigation, margins, and header.
#[derive(Clone, Copy, Debug)]
pub struct SolvingLayout {
    pub board_size: f32,
    pub board_column: f32,
    pub panel_width: f32,
}
pub fn solving(width: f32, height: f32, mode: LayoutMode, panel: f32) -> SolvingLayout {
    let width = if width.is_finite() {
        width.max(0.0)
    } else {
        0.0
    };
    let height = if height.is_finite() {
        height.max(0.0)
    } else {
        0.0
    };
    if mode == LayoutMode::Columns {
        let panel_width = panel.max(0.0).min(width * 0.43);
        let board_column = (width - panel_width - 24.0).max(0.0);
        SolvingLayout {
            board_size: board_column.min((height - 42.0).max(0.0)),
            board_column,
            panel_width,
        }
    } else {
        SolvingLayout {
            board_size: width.min((height - 30.0).max(0.0)),
            board_column: width,
            panel_width: width,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn content_geometry_respects_both_columns_and_stack_height() {
        for (w, h) in [
            (360.0, 640.0),
            (640.0, 900.0),
            (960.0, 1080.0),
            (1920.0, 1080.0),
            (3840.0, 2160.0),
        ] {
            let l = calculate(w, h);
            let available = w - l.navigation_width - 56.0;
            let g = solving(available, h - 140.0, l.mode, l.panel_width);
            assert!(g.board_size <= available);
            assert!(g.board_size <= h - 140.0);
            if l.mode == LayoutMode::Columns {
                assert!(g.board_column + g.panel_width + 24.0 <= available + 0.01);
            }
        }
        assert_eq!(
            solving(f32::NAN, 0.0, LayoutMode::Stacked, 320.0).board_size,
            0.0
        );
    }
    #[test]
    fn breakpoints() {
        assert_eq!(calculate(899.0, 900.0).mode, LayoutMode::Stacked);
        assert_eq!(calculate(900.0, 900.0).mode, LayoutMode::Columns);
        assert_eq!(calculate(360.0, 800.0).navigation, Navigation::TopBar);
        assert_eq!(calculate(800.0, 800.0).navigation, Navigation::IconRail);
        assert_eq!(calculate(3840.0, 2160.0).navigation, Navigation::Sidebar);
    }
    #[test]
    fn hyprland_tiles_and_hidpi_logical_size() {
        for (w, h, nav, mode) in [
            (360.0, 640.0, Navigation::TopBar, LayoutMode::Stacked),
            (640.0, 900.0, Navigation::IconRail, LayoutMode::Stacked),
            (960.0, 1080.0, Navigation::IconRail, LayoutMode::Columns),
            (1920.0, 1080.0, Navigation::Sidebar, LayoutMode::Columns),
            (3840.0, 2160.0, Navigation::Sidebar, LayoutMode::Columns),
        ] {
            let layout = calculate(w, h);
            assert_eq!(layout.navigation, nav);
            assert_eq!(layout.mode, mode);
            assert!(layout.board_size <= w - layout.navigation_width);
            assert!(layout.board_size <= h - 96.0);
        }
        let physical = calculate(3840.0 / 2.0, 2160.0 / 2.0);
        assert_eq!(physical.navigation, Navigation::Sidebar);
    }
    #[test]
    fn geometry_never_overflows() {
        for width in [0.0, 100.0, 360.0, 600.0, 899.0, 900.0, 1200.0, 3840.0] {
            for height in [0.0, 80.0, 300.0, 800.0, 2160.0] {
                let l = calculate(width, height);
                assert!(l.board_size >= 0.0);
                assert!(l.board_size <= width);
                assert!(l.board_size <= (height - 96.0).max(0.0));
                if l.mode == LayoutMode::Columns {
                    assert!(l.board_size + l.panel_width + l.navigation_width + 56.0 <= width);
                }
            }
        }
        assert_eq!(calculate(f32::NAN, f32::INFINITY).board_size, 0.0);
    }
}
