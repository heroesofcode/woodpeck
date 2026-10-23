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
    let navigation = if width < 600.0 {
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
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn breakpoints() {
        assert_eq!(calculate(899.0, 900.0).mode, LayoutMode::Stacked);
        assert_eq!(calculate(900.0, 900.0).mode, LayoutMode::Columns);
        assert_eq!(calculate(360.0, 800.0).navigation, Navigation::TopBar);
        assert_eq!(calculate(800.0, 800.0).navigation, Navigation::IconRail);
        assert_eq!(calculate(3840.0, 2160.0).navigation, Navigation::Sidebar);
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
