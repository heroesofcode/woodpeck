//! Context-dependent keyboard dispatch, independent of egui input state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    S,
    T,
    Left,
    Right,
    Space,
    Enter,
    Escape,
    Q,
    R,
    B,
    N,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Skip,
    Timer,
    Previous,
    NextMove,
    Advance,
    Close,
    Promote(char),
}
#[derive(Clone, Copy, Default)]
pub struct Context {
    pub editing: bool,
    pub modified: bool,
    pub blocked: bool,
    pub walkthrough: bool,
    pub advance: bool,
    pub promotion: bool,
}
pub fn dispatch(key: Key, context: Context) -> Option<Action> {
    let Context {
        editing,
        modified,
        blocked,
        walkthrough,
        advance,
        promotion,
    } = context;
    if modified || editing {
        return None;
    }
    if key == Key::Escape {
        return Some(Action::Close);
    }
    if promotion {
        return match key {
            Key::Q => Some(Action::Promote('q')),
            Key::R => Some(Action::Promote('r')),
            Key::B => Some(Action::Promote('b')),
            Key::N => Some(Action::Promote('n')),
            _ => None,
        };
    }
    if blocked {
        return None;
    }
    match key {
        Key::S => Some(Action::Skip),
        Key::T => Some(Action::Timer),
        Key::Left if walkthrough => Some(Action::Previous),
        Key::Right | Key::Space if walkthrough => Some(Action::NextMove),
        Key::Enter | Key::Space if advance => Some(Action::Advance),
        _ => None,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn contexts() {
        assert_eq!(
            dispatch(
                Key::Space,
                Context {
                    editing: false,
                    modified: false,
                    blocked: false,
                    walkthrough: true,
                    advance: false,
                    promotion: false
                }
            ),
            Some(Action::NextMove)
        );
        assert_eq!(
            dispatch(
                Key::Space,
                Context {
                    editing: false,
                    modified: false,
                    blocked: false,
                    walkthrough: false,
                    advance: true,
                    promotion: false
                }
            ),
            Some(Action::Advance)
        );
        assert_eq!(
            dispatch(
                Key::Q,
                Context {
                    editing: false,
                    modified: false,
                    blocked: false,
                    walkthrough: false,
                    advance: false,
                    promotion: true
                }
            ),
            Some(Action::Promote('q'))
        );
        assert_eq!(
            dispatch(
                Key::S,
                Context {
                    editing: false,
                    modified: false,
                    blocked: false,
                    walkthrough: false,
                    advance: false,
                    promotion: false
                }
            ),
            Some(Action::Skip)
        );
        assert_eq!(
            dispatch(
                Key::Left,
                Context {
                    editing: false,
                    modified: false,
                    blocked: false,
                    walkthrough: true,
                    advance: false,
                    promotion: false
                }
            ),
            Some(Action::Previous)
        );
        assert_eq!(
            dispatch(
                Key::Escape,
                Context {
                    editing: false,
                    modified: false,
                    blocked: true,
                    walkthrough: false,
                    advance: false,
                    promotion: false
                }
            ),
            Some(Action::Close)
        );
    }
    #[test]
    fn editing_modifiers_and_menus_suppress_actions() {
        for key in [Key::S, Key::T, Key::Space, Key::Enter, Key::Q] {
            assert_eq!(
                dispatch(
                    key,
                    Context {
                        editing: true,
                        modified: false,
                        blocked: false,
                        walkthrough: true,
                        advance: true,
                        promotion: true
                    }
                ),
                None
            );
            assert_eq!(
                dispatch(
                    key,
                    Context {
                        editing: false,
                        modified: true,
                        blocked: false,
                        walkthrough: true,
                        advance: true,
                        promotion: true
                    }
                ),
                None
            );
            assert_eq!(
                dispatch(
                    key,
                    Context {
                        editing: false,
                        modified: false,
                        blocked: true,
                        walkthrough: true,
                        advance: true,
                        promotion: false
                    }
                ),
                None
            );
        }
    }
}
