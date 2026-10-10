use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::state::Action;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InputMode {
    Normal,
    Picker,
    Confirm,
}

pub fn action_for(key: KeyEvent, mode: InputMode) -> Option<Action> {
    match mode {
        InputMode::Picker => return picker_action(key),
        InputMode::Confirm => {
            return Some(Action::Confirm(matches!(
                key.code,
                KeyCode::Char('y' | 'Y')
            )));
        }
        InputMode::Normal => {}
    }
    let control = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Char('d') if control => Some(Action::HalfDown),
        KeyCode::Char('u') if control => Some(Action::HalfUp),
        KeyCode::Char('c') if control => Some(Action::Quit),
        KeyCode::Char('j') | KeyCode::Down => Some(Action::Down),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::Up),
        KeyCode::Char('g') | KeyCode::Home => Some(Action::Top),
        KeyCode::Char('G') | KeyCode::End => Some(Action::Bottom),
        KeyCode::PageDown => Some(Action::HalfDown),
        KeyCode::PageUp => Some(Action::HalfUp),
        KeyCode::Char(']') => Some(Action::NextRef),
        KeyCode::Char('[') => Some(Action::PrevRef),
        KeyCode::Char('h') => Some(Action::Head),
        KeyCode::Char('b') => Some(Action::OpenPicker),
        KeyCode::Char('d') => Some(Action::ToggleDetail),
        KeyCode::Tab => Some(Action::FocusNext),
        KeyCode::Char(' ') => Some(Action::Checkout),
        KeyCode::Char('/') => Some(Action::OpenFinder),
        KeyCode::Char('?') => Some(Action::ToggleHelp),
        KeyCode::Char('R') => Some(Action::ResetMenu),
        KeyCode::Char('r') => Some(Action::Rebase),
        KeyCode::Char('x') => Some(Action::AbortRebase),
        KeyCode::Char('f') => Some(Action::Fetch),
        KeyCode::Enter => Some(Action::Open),
        KeyCode::Esc | KeyCode::Backspace => Some(Action::Back),
        KeyCode::Char('q') => Some(Action::Quit),
        _ => None,
    }
}

fn picker_action(key: KeyEvent) -> Option<Action> {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Some(Action::Quit);
    }
    match key.code {
        KeyCode::Esc => Some(Action::Back),
        KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => {
            Some(Action::PickerCheckout)
        }
        KeyCode::Enter => Some(Action::PickerConfirm),
        KeyCode::Backspace => Some(Action::PickerBackspace),
        KeyCode::Down => Some(Action::PickerDown),
        KeyCode::Up => Some(Action::PickerUp),
        KeyCode::Char(character) => Some(Action::PickerChar(character)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn graph_keys_map_to_navigation() {
        assert_eq!(
            action_for(key(KeyCode::Char('j')), InputMode::Normal),
            Some(Action::Down)
        );
        assert_eq!(
            action_for(key(KeyCode::Up), InputMode::Normal),
            Some(Action::Up)
        );
        assert_eq!(
            action_for(key(KeyCode::Char('G')), InputMode::Normal),
            Some(Action::Bottom)
        );
        assert_eq!(
            action_for(
                KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL),
                InputMode::Normal
            ),
            Some(Action::HalfDown)
        );
        assert_eq!(
            action_for(key(KeyCode::Char(']')), InputMode::Normal),
            Some(Action::NextRef)
        );
        assert_eq!(
            action_for(key(KeyCode::Char('b')), InputMode::Normal),
            Some(Action::OpenPicker)
        );
        assert_eq!(
            action_for(key(KeyCode::Char('h')), InputMode::Normal),
            Some(Action::Head)
        );
        assert_eq!(
            action_for(key(KeyCode::Tab), InputMode::Normal),
            Some(Action::FocusNext)
        );
        assert_eq!(
            action_for(key(KeyCode::Char('d')), InputMode::Normal),
            Some(Action::ToggleDetail)
        );
        assert_eq!(
            action_for(key(KeyCode::Backspace), InputMode::Normal),
            Some(Action::Back)
        );
        assert_eq!(
            action_for(key(KeyCode::Char('q')), InputMode::Normal),
            Some(Action::Quit)
        );
    }

    #[test]
    fn space_checks_out_and_a_confirmation_only_accepts_y() {
        assert_eq!(
            action_for(key(KeyCode::Char(' ')), InputMode::Normal),
            Some(Action::Checkout)
        );
        assert_eq!(
            action_for(key(KeyCode::Char('y')), InputMode::Confirm),
            Some(Action::Confirm(true))
        );
        assert_eq!(
            action_for(key(KeyCode::Char('n')), InputMode::Confirm),
            Some(Action::Confirm(false))
        );
        assert_eq!(
            action_for(key(KeyCode::Enter), InputMode::Confirm),
            Some(Action::Confirm(false))
        );
    }

    #[test]
    fn shift_enter_in_a_list_checks_out_the_entry() {
        assert_eq!(
            action_for(
                KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT),
                InputMode::Picker
            ),
            Some(Action::PickerCheckout)
        );
    }

    #[test]
    fn question_mark_opens_the_help() {
        assert_eq!(
            action_for(key(KeyCode::Char('?')), InputMode::Normal),
            Some(Action::ToggleHelp)
        );
    }

    #[test]
    fn reset_rebase_and_abort_keys() {
        assert_eq!(
            action_for(key(KeyCode::Char('R')), InputMode::Normal),
            Some(Action::ResetMenu)
        );
        assert_eq!(
            action_for(key(KeyCode::Char('r')), InputMode::Normal),
            Some(Action::Rebase)
        );
        assert_eq!(
            action_for(key(KeyCode::Char('x')), InputMode::Normal),
            Some(Action::AbortRebase)
        );
    }

    #[test]
    fn f_fetches() {
        assert_eq!(
            action_for(key(KeyCode::Char('f')), InputMode::Normal),
            Some(Action::Fetch)
        );
    }

    #[test]
    fn slash_opens_the_finder() {
        assert_eq!(
            action_for(key(KeyCode::Char('/')), InputMode::Normal),
            Some(Action::OpenFinder)
        );
    }

    #[test]
    fn picker_types_letters_and_moves_with_arrows() {
        assert_eq!(
            action_for(key(KeyCode::Char('j')), InputMode::Picker),
            Some(Action::PickerChar('j'))
        );
        assert_eq!(
            action_for(key(KeyCode::Char(' ')), InputMode::Picker),
            Some(Action::PickerChar(' '))
        );
        assert_eq!(
            action_for(key(KeyCode::Down), InputMode::Picker),
            Some(Action::PickerDown)
        );
        assert_eq!(
            action_for(key(KeyCode::Backspace), InputMode::Picker),
            Some(Action::PickerBackspace)
        );
        assert_eq!(
            action_for(key(KeyCode::Esc), InputMode::Picker),
            Some(Action::Back)
        );
        assert_eq!(
            action_for(key(KeyCode::Enter), InputMode::Picker),
            Some(Action::PickerConfirm)
        );
    }
}
