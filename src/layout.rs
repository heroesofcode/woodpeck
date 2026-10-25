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
    let margins = if width < 640.0 { 28.0 } else { 56.0 };
    let available = (width - navigation_width - margins).max(0.0);
    let mode = if width < 900.0 || height > width {
        LayoutMode::Stacked
    } else {
        LayoutMode::Columns
    };
    let panel_width = if mode == LayoutMode::Columns {
        if width >= 1200.0 { 320.0 } else { 288.0 }
    } else {
        available
    };
    let navigation_height = if navigation == Navigation::TopBar {
        88.0
    } else {
        0.0
    };
    let content_height = (height - margins - navigation_height - 30.0).max(0.0);
    let geometry = solving(available, content_height, mode, panel_width, 112.0);
    Layout {
        mode,
        navigation,
        board_size: geometry.board_size,
        panel_width,
        navigation_width,
    }
}
/// Geometry within the content area after the navigation, margins, and header.
/// The action rectangle never belongs to a scroll area. Only `details_height` may scroll.
#[derive(Clone, Copy, Debug)]
pub struct SolvingLayout {
    pub board_size: f32,
    pub board_column: f32,
    pub panel_width: f32,
    pub panel_height: f32,
    pub details_height: f32,
    pub action_height: f32,
    pub compact: bool,
    pub gap: f32,
}
pub const BOARD_FOOTER: f32 = 24.0;

pub fn solving(
    width: f32,
    height: f32,
    mode: LayoutMode,
    panel: f32,
    actions: f32,
) -> SolvingLayout {
    let finite = |v: f32| if v.is_finite() { v.max(0.0) } else { 0.0 };
    let (width, height) = (finite(width), finite(height));
    let compact = mode == LayoutMode::Stacked || height < 700.0;
    let action_height = finite(actions).min(height);
    let gap = (if compact { 10.0_f32 } else { 24.0 })
        .min(width)
        .min((height - action_height).max(0.0));
    if mode == LayoutMode::Columns {
        let panel_width = finite(panel).min(width * 0.43);
        let board_column = (width - panel_width - gap).max(0.0);
        SolvingLayout {
            board_size: board_column.min((height - BOARD_FOOTER).max(0.0)),
            board_column,
            panel_width,
            panel_height: height,
            details_height: (height - action_height - gap).max(0.0),
            action_height,
            compact,
            gap,
        }
    } else {
        // Keep a timer/progress summary visible even in the smallest portrait tile.
        let details_height = (if height < 700.0 { 72.0_f32 } else { 140.0 })
            .min((height - action_height - gap).max(0.0));
        let panel_height = details_height + action_height + gap;
        SolvingLayout {
            board_size: width.min((height - panel_height - gap - BOARD_FOOTER).max(0.0)),
            board_column: width,
            panel_width: width,
            panel_height,
            details_height,
            action_height,
            compact,
            gap,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    pub const SIZES: [(f32, f32); 9] = [
        (1280.0, 800.0),
        (1920.0, 1080.0),
        (1366.0, 768.0),
        (900.0, 600.0),
        (600.0, 1000.0),
        (360.0, 640.0),
        (640.0, 900.0),
        (960.0, 1080.0),
        (3840.0, 2160.0),
    ];
    #[test]
    fn board_and_pinned_actions_fit_all_requested_sizes_and_states() {
        for (w, h) in SIZES {
            let l = calculate(w, h);
            let margin = if w < 640.0 { 28.0 } else { 56.0 };
            let nav_height = if l.navigation == Navigation::TopBar {
                88.0
            } else {
                0.0
            };
            let available = w - l.navigation_width - margin;
            let height = h - nav_height - margin - 30.0;
            // Playing, solved, walkthrough, and walkthrough with a save retry.
            for actions in [112.0, 128.0, 180.0, 232.0] {
                let g = solving(available, height, l.mode, l.panel_width, actions);
                assert!(g.board_size > 0.0, "{w}x{h}");
                assert!(g.board_size <= available);
                assert_eq!(g.action_height, actions);
                assert!(g.details_height + g.gap + actions <= g.panel_height + 0.01);
                if l.mode == LayoutMode::Columns {
                    assert!(g.board_column + g.panel_width + g.gap <= available + 0.01);
                    assert!(g.board_size + BOARD_FOOTER <= height + 0.01);
                    assert!(g.panel_height <= height);
                } else {
                    assert!(g.board_size + BOARD_FOOTER + g.gap + g.panel_height <= height + 0.01);
                }
            }
        }
    }
    #[test]
    fn breakpoints_and_compact_mode() {
        assert_eq!(calculate(899.0, 900.0).mode, LayoutMode::Stacked);
        assert_eq!(calculate(900.0, 600.0).mode, LayoutMode::Columns);
        assert_eq!(calculate(960.0, 1080.0).mode, LayoutMode::Stacked);
        assert_eq!(calculate(360.0, 640.0).navigation, Navigation::TopBar);
        assert_eq!(calculate(900.0, 600.0).navigation, Navigation::IconRail);
        assert_eq!(calculate(1920.0, 1080.0).navigation, Navigation::Sidebar);
        assert!(solving(800.0, 510.0, LayoutMode::Columns, 288.0, 112.0).compact);
        assert!(!solving(1600.0, 900.0, LayoutMode::Columns, 320.0, 112.0).compact);
    }
    #[test]
    fn geometry_handles_degenerate_input() {
        for width in [0.0, 100.0, 360.0, 900.0, f32::NAN] {
            for height in [0.0, 80.0, 800.0, f32::INFINITY] {
                for mode in [LayoutMode::Stacked, LayoutMode::Columns] {
                    let g = solving(width, height, mode, 320.0, 180.0);
                    assert!(g.board_size.is_finite() && g.board_size >= 0.0);
                    assert!(g.action_height.is_finite() && g.action_height >= 0.0);
                }
            }
        }
    }
}
