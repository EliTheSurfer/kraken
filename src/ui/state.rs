use std::collections::HashMap;

use crate::git::{Commit, Head, Refs, ResetMode, ref_badges};
use crate::graph::{Layout, Row};
use crate::ui::finder::{Entry, EntryKind, rank};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Focus {
    Graph,
    Detail,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Down,
    Up,
    Top,
    Bottom,
    HalfDown,
    HalfUp,
    NextRef,
    PrevRef,
    Head,
    OpenPicker,
    ToggleDetail,
    FocusNext,
    Back,
    Quit,
    PickerChar(char),
    PickerBackspace,
    PickerDown,
    PickerUp,
    PickerConfirm,
    Open,
    Checkout,
    Confirm(bool),
    OpenFinder,
    PickerCheckout,
    ToggleHelp,
    ResetMenu,
    Rebase,
    AbortRebase,
    Fetch,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PickerPurpose {
    Jump,
    Find,
    Checkout,
    Reset,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PendingAction {
    Checkout(String),
    Reset { mode: ResetMode, hash: String },
    Rebase { onto: String },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Confirmation {
    pub message: String,
    pub action: PendingAction,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    None,
    Quit,
    NeedMore,
    LoadDetail(String),
    Review { hash: String },
    Checkout { name: String },
    Confirmed(PendingAction),
    Request(PendingAction),
    AbortRebase,
    Fetch,
}

#[derive(Clone, Debug, PartialEq)]
enum Pending {
    Bottom,
    Hash(String),
    Restore(String),
}

pub struct RefPicker {
    pub filter: String,
    pub cursor: usize,
    pub items: Vec<Entry>,
    pub purpose: PickerPurpose,
}

fn ref_entry(reference: &crate::git::Ref) -> Entry {
    let kind = match reference.kind {
        crate::git::RefKind::Local => EntryKind::Local,
        crate::git::RefKind::Remote { .. } => EntryKind::Remote,
        crate::git::RefKind::Tag => EntryKind::Tag,
    };
    Entry {
        kind,
        label: reference.name.clone(),
        detail: String::new(),
        target: reference.target.clone(),
    }
}

fn commit_entry(commit: &Commit) -> Entry {
    let short: String = commit.hash.chars().take(7).collect();
    Entry {
        kind: EntryKind::Commit,
        label: commit.subject.clone(),
        detail: format!("{short} · {}", commit.author.name),
        target: commit.hash.clone(),
    }
}

fn index_refs(refs: &Refs) -> HashMap<String, Vec<usize>> {
    let mut refs_by_hash: HashMap<String, Vec<usize>> = HashMap::new();
    for (index, reference) in refs.refs.iter().enumerate() {
        refs_by_hash
            .entry(reference.target.clone())
            .or_default()
            .push(index);
    }
    refs_by_hash
}

impl RefPicker {
    pub fn visible(&self) -> Vec<(&Entry, Vec<usize>)> {
        rank(&self.filter, &self.items)
            .into_iter()
            .map(|ranked| (&self.items[ranked.entry], ranked.label_matches))
            .collect()
    }

    fn selected(&self) -> Option<Entry> {
        self.visible()
            .get(self.cursor)
            .map(|(entry, _)| (*entry).clone())
    }
}

pub struct App {
    pub commits: Vec<Commit>,
    pub rows: Vec<Row>,
    pub refs: Refs,
    pub selected: usize,
    pub offset: usize,
    pub focus: Focus,
    pub show_detail: bool,
    pub file_count: usize,
    pub file_cursor: usize,
    pub picker: Option<RefPicker>,
    pub confirm: Option<Confirmation>,
    pub help_open: bool,
    pub help_scroll: usize,
    pub sync_note: String,
    pub rebasing: bool,
    pub status: String,
    pub loading_done: bool,
    pub lane_cells: u16,
    pub row_lines: u16,
    pub panel_visible: bool,
    layout: Layout,
    index_by_hash: HashMap<String, usize>,
    refs_by_hash: HashMap<String, Vec<usize>>,
    pending: Option<Pending>,
}

impl App {
    pub fn new(refs: Refs) -> App {
        let refs_by_hash = index_refs(&refs);
        App {
            commits: Vec::new(),
            rows: Vec::new(),
            refs,
            selected: 0,
            offset: 0,
            focus: Focus::Graph,
            show_detail: true,
            file_count: 0,
            file_cursor: 0,
            picker: None,
            confirm: None,
            help_open: false,
            help_scroll: 0,
            sync_note: String::new(),
            rebasing: false,
            status: String::new(),
            loading_done: false,
            lane_cells: 2,
            row_lines: 1,
            panel_visible: true,
            layout: Layout::new(),
            index_by_hash: HashMap::new(),
            refs_by_hash,
            pending: None,
        }
    }

    pub fn add_commits(&mut self, commits: Vec<Commit>, page: usize) -> Effect {
        for commit in commits {
            self.rows
                .push(self.layout.push(&commit.hash, &commit.parents));
            self.index_by_hash
                .insert(commit.hash.clone(), self.commits.len());
            self.commits.push(commit);
        }
        self.resolve_pending(page)
    }

    pub fn finish_loading(&mut self) {
        self.loading_done = true;
    }

    pub fn resolve_pending(&mut self, page: usize) -> Effect {
        match self.pending.clone() {
            Some(Pending::Bottom) if self.loading_done => {
                self.pending = None;
                self.select(self.commits.len().saturating_sub(1), page)
            }
            Some(Pending::Hash(hash)) => match self.index_by_hash.get(&hash).copied() {
                Some(index) => {
                    self.pending = None;
                    self.select(index, page)
                }
                None if self.loading_done => {
                    self.pending = None;
                    self.status = "commit not found in history".to_string();
                    Effect::None
                }
                None => Effect::NeedMore,
            },
            Some(Pending::Restore(hash)) => match self.index_by_hash.get(&hash).copied() {
                Some(index) => {
                    self.pending = None;
                    self.select(index, page)
                }
                None if self.loading_done => {
                    self.pending = None;
                    self.focus_head(page)
                }
                None => Effect::NeedMore,
            },
            Some(Pending::Bottom) => Effect::NeedMore,
            None => Effect::None,
        }
    }

    pub fn refs_at(&self, index: usize) -> Vec<&crate::git::Ref> {
        let Some(commit) = self.commits.get(index) else {
            return Vec::new();
        };
        self.refs_by_hash
            .get(&commit.hash)
            .map(|indexes| {
                indexes
                    .iter()
                    .map(|&ref_index| &self.refs.refs[ref_index])
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn row_height(&self, index: usize) -> u16 {
        let base = self.row_lines.max(1);
        if index != self.selected {
            return base;
        }
        base.max(ref_badges(&self.refs_at(index)).len() as u16)
    }

    pub fn head_hash(&self) -> Option<String> {
        match &self.refs.head {
            Head::Detached(hash) => Some(hash.clone()),
            Head::Branch(name) => self
                .refs
                .refs
                .iter()
                .find(|reference| {
                    &reference.name == name && reference.kind == crate::git::RefKind::Local
                })
                .map(|reference| reference.target.clone()),
        }
    }

    fn rebase_target(&self, hash: &str) -> String {
        let index = self
            .index_by_hash
            .get(hash)
            .copied()
            .unwrap_or(self.selected);
        let refs = self.refs_at(index);
        refs.iter()
            .find(|reference| reference.kind == crate::git::RefKind::Local)
            .or_else(|| {
                refs.iter()
                    .find(|reference| matches!(reference.kind, crate::git::RefKind::Remote { .. }))
            })
            .map_or_else(|| hash.to_string(), |reference| reference.name.clone())
    }

    pub fn restore_selection(&mut self, hash: String, page: usize) -> Effect {
        self.pending = Some(Pending::Restore(hash));
        self.resolve_pending(page.max(1))
    }

    pub fn clear_history(&mut self) {
        self.commits.clear();
        self.rows.clear();
        self.layout = Layout::new();
        self.index_by_hash.clear();
        self.selected = 0;
        self.offset = 0;
        self.loading_done = false;
        self.pending = None;
        self.file_cursor = 0;
    }

    pub fn focus_head(&mut self, page: usize) -> Effect {
        match self.head_hash() {
            Some(hash) => self.jump_to(hash, page.max(1)),
            None => Effect::None,
        }
    }

    pub fn set_refs(&mut self, refs: Refs) {
        self.refs_by_hash = index_refs(&refs);
        self.refs = refs;
    }

    pub fn apply(&mut self, action: Action, page: usize) -> Effect {
        let page = page.max(1);
        if let Some(confirmation) = self.confirm.take() {
            return match action {
                Action::Confirm(true) => Effect::Confirmed(confirmation.action),
                _ => {
                    self.status = "cancelled".to_string();
                    Effect::None
                }
            };
        }
        if self.help_open {
            match action {
                Action::ToggleHelp | Action::Back | Action::Quit => self.help_open = false,
                Action::Down => self.help_scroll += 1,
                Action::Up => self.help_scroll = self.help_scroll.saturating_sub(1),
                _ => {}
            }
            return Effect::None;
        }
        if action == Action::ToggleHelp {
            self.help_open = true;
            self.help_scroll = 0;
            return Effect::None;
        }
        if self.picker.is_some() {
            return self.apply_picker(action, page);
        }
        if self.focus == Focus::Detail
            && let Some(effect) = self.apply_panel(&action, page)
        {
            return effect;
        }
        if matches!(
            action,
            Action::Down
                | Action::Up
                | Action::Top
                | Action::HalfDown
                | Action::HalfUp
                | Action::NextRef
                | Action::PrevRef
        ) {
            self.pending = None;
        }
        match action {
            Action::Down => self.select(self.selected + 1, page),
            Action::Up => self.select(self.selected.saturating_sub(1), page),
            Action::Top => self.select(0, page),
            Action::Bottom => {
                self.pending = Some(Pending::Bottom);
                self.resolve_pending(page)
            }
            Action::HalfDown => self.select(self.selected + page / 2, page),
            Action::HalfUp => self.select(self.selected.saturating_sub(page / 2), page),
            Action::NextRef => match (self.selected + 1..self.commits.len())
                .find(|&index| !self.refs_at(index).is_empty())
            {
                Some(index) => self.select(index, page),
                None => Effect::None,
            },
            Action::PrevRef => match (0..self.selected)
                .rev()
                .find(|&index| !self.refs_at(index).is_empty())
            {
                Some(index) => self.select(index, page),
                None => Effect::None,
            },
            Action::Head => match self.head_hash() {
                Some(hash) => self.jump_to(hash, page),
                None => Effect::None,
            },
            Action::OpenPicker => {
                let items = self.refs.refs.iter().map(ref_entry).collect();
                self.open_picker(items, PickerPurpose::Jump);
                Effect::None
            }
            Action::ResetMenu => {
                let Some(commit) = self.commits.get(self.selected) else {
                    return Effect::None;
                };
                let items = [
                    ("soft", "keep the changes staged"),
                    ("mixed", "keep the changes unstaged"),
                    ("hard", "discard every change"),
                ]
                .iter()
                .map(|(mode, detail)| Entry {
                    kind: EntryKind::Action,
                    label: format!("{mode} reset"),
                    detail: detail.to_string(),
                    target: commit.hash.clone(),
                })
                .collect();
                self.open_picker(items, PickerPurpose::Reset);
                Effect::None
            }
            Action::Rebase => match self.commits.get(self.selected) {
                Some(commit) => Effect::Request(PendingAction::Rebase {
                    onto: self.rebase_target(&commit.hash),
                }),
                None => Effect::None,
            },
            Action::AbortRebase if self.rebasing => Effect::AbortRebase,
            Action::Fetch => Effect::Fetch,
            Action::OpenFinder => {
                let mut items: Vec<Entry> = self.refs.refs.iter().map(ref_entry).collect();
                items.extend(self.commits.iter().map(commit_entry));
                self.open_picker(items, PickerPurpose::Find);
                Effect::None
            }
            Action::Checkout => {
                let branches: Vec<Entry> = self
                    .refs_at(self.selected)
                    .into_iter()
                    .filter(|reference| reference.kind != crate::git::RefKind::Tag)
                    .map(ref_entry)
                    .collect();
                match branches.len() {
                    0 => {
                        self.status = "no branch on this commit".to_string();
                        Effect::None
                    }
                    1 => Effect::Checkout {
                        name: branches[0].label.clone(),
                    },
                    _ => {
                        self.open_picker(branches, PickerPurpose::Checkout);
                        Effect::None
                    }
                }
            }
            Action::ToggleDetail => {
                self.show_detail = !self.show_detail;
                self.focus = Focus::Graph;
                Effect::None
            }
            Action::Open => self.review(),
            Action::FocusNext => {
                if self.show_detail && self.panel_visible {
                    self.focus = Focus::Detail;
                }
                Effect::None
            }
            Action::Quit => Effect::Quit,
            _ => Effect::None,
        }
    }

    fn apply_panel(&mut self, action: &Action, _page: usize) -> Option<Effect> {
        match action {
            Action::Down => {
                self.file_cursor = (self.file_cursor + 1).min(self.file_count.saturating_sub(1))
            }
            Action::Up => self.file_cursor = self.file_cursor.saturating_sub(1),
            Action::Open => return Some(self.review()),
            Action::FocusNext | Action::Back => self.focus = Focus::Graph,
            _ => return None,
        }
        Some(Effect::None)
    }

    fn review(&self) -> Effect {
        match self.commits.get(self.selected) {
            Some(commit) => Effect::Review {
                hash: commit.hash.clone(),
            },
            None => Effect::None,
        }
    }

    fn open_picker(&mut self, items: Vec<Entry>, purpose: PickerPurpose) {
        self.picker = Some(RefPicker {
            filter: String::new(),
            cursor: 0,
            items,
            purpose,
        });
    }

    fn apply_picker(&mut self, action: Action, page: usize) -> Effect {
        let Some(picker) = self.picker.as_mut() else {
            return Effect::None;
        };
        match action {
            Action::Back => self.picker = None,
            Action::PickerChar(character) => {
                picker.filter.push(character);
                picker.cursor = 0;
            }
            Action::PickerBackspace => {
                picker.filter.pop();
                picker.cursor = 0;
            }
            Action::PickerDown => {
                picker.cursor = (picker.cursor + 1).min(picker.visible().len().saturating_sub(1))
            }
            Action::PickerUp => picker.cursor = picker.cursor.saturating_sub(1),
            Action::PickerCheckout => {
                let Some(entry) = picker.selected() else {
                    return Effect::None;
                };
                if !matches!(entry.kind, EntryKind::Local | EntryKind::Remote) {
                    self.status = "pick a branch to check out".to_string();
                    return Effect::None;
                }
                self.picker = None;
                return Effect::Checkout { name: entry.label };
            }
            Action::PickerConfirm if picker.purpose == PickerPurpose::Reset => {
                let Some(entry) = picker.selected() else {
                    return Effect::None;
                };
                self.picker = None;
                let mode = match entry.label.split_whitespace().next() {
                    Some("soft") => ResetMode::Soft,
                    Some("mixed") => ResetMode::Mixed,
                    _ => ResetMode::Hard,
                };
                return Effect::Request(PendingAction::Reset {
                    mode,
                    hash: entry.target,
                });
            }
            Action::PickerConfirm if picker.purpose == PickerPurpose::Checkout => {
                let name = picker.selected().map(|entry| entry.label);
                self.picker = None;
                if let Some(name) = name {
                    return Effect::Checkout { name };
                }
            }
            Action::PickerConfirm => {
                let target = picker.selected().map(|entry| entry.target);
                self.picker = None;
                if let Some(hash) = target {
                    return self.jump_to(hash, page);
                }
            }
            Action::Quit => return Effect::Quit,
            _ => {}
        }
        Effect::None
    }

    fn jump_to(&mut self, hash: String, page: usize) -> Effect {
        self.pending = Some(Pending::Hash(hash));
        self.resolve_pending(page)
    }

    fn select(&mut self, index: usize, page: usize) -> Effect {
        if self.commits.is_empty() {
            return Effect::None;
        }
        let index = index.min(self.commits.len() - 1);
        let changed = index != self.selected;
        self.selected = index;
        if self.selected < self.offset {
            self.offset = self.selected;
        }
        if self.selected >= self.offset + page {
            self.offset = self.selected + 1 - page;
        }
        if !changed {
            return Effect::None;
        }
        self.file_cursor = 0;
        Effect::LoadDetail(self.commits[index].hash.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::{Person, Ref, RefKind, ResetMode};

    fn commit(hash: &str, parents: &[&str]) -> Commit {
        let person = Person {
            name: "Ada".to_string(),
            email: "ada@example.com".to_string(),
            time: 0,
        };
        Commit {
            hash: hash.to_string(),
            parents: parents.iter().map(|parent| parent.to_string()).collect(),
            author: person.clone(),
            committer: person,
            subject: format!("subject {hash}"),
        }
    }

    fn reference(name: &str, target: &str) -> Ref {
        Ref {
            name: name.to_string(),
            kind: RefKind::Local,
            target: target.to_string(),
        }
    }

    fn app_with(count: usize, refs: Vec<Ref>, head: &str) -> App {
        let mut app = App::new(Refs {
            refs,
            head: Head::Branch(head.to_string()),
        });
        let commits = (0..count)
            .map(|index| {
                let hash = format!("c{index}");
                let parent = format!("c{}", index + 1);
                if index + 1 == count {
                    commit(&hash, &[])
                } else {
                    commit(&hash, &[parent.as_str()])
                }
            })
            .collect();
        app.add_commits(commits, 10);
        app
    }

    #[test]
    fn down_and_up_move_within_bounds_and_load_the_detail() {
        let mut app = app_with(3, vec![], "main");
        assert_eq!(
            app.apply(Action::Down, 10),
            Effect::LoadDetail("c1".to_string())
        );
        app.apply(Action::Down, 10);
        app.apply(Action::Down, 10);
        assert_eq!(app.selected, 2);
        app.apply(Action::Up, 10);
        app.apply(Action::Up, 10);
        app.apply(Action::Up, 10);
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn offset_follows_the_selection_past_the_page() {
        let mut app = app_with(30, vec![], "main");
        for _ in 0..12 {
            app.apply(Action::Down, 10);
        }
        assert_eq!((app.selected, app.offset), (12, 3));
        app.apply(Action::Top, 10);
        assert_eq!((app.selected, app.offset), (0, 0));
    }

    #[test]
    fn next_and_previous_ref_jump_between_labelled_commits() {
        let mut app = app_with(
            10,
            vec![
                reference("main", "c0"),
                reference("feature", "c4"),
                reference("v1", "c7"),
            ],
            "main",
        );
        app.apply(Action::NextRef, 10);
        assert_eq!(app.selected, 4);
        app.apply(Action::NextRef, 10);
        assert_eq!(app.selected, 7);
        app.apply(Action::PrevRef, 10);
        assert_eq!(app.selected, 4);
    }

    #[test]
    fn head_jumps_to_the_checked_out_branch() {
        let mut app = app_with(10, vec![reference("main", "c5")], "main");
        app.apply(Action::Head, 10);
        assert_eq!(app.selected, 5);
    }

    #[test]
    fn picker_filters_case_insensitively_and_confirm_jumps() {
        let mut app = app_with(
            10,
            vec![reference("main", "c0"), reference("Feature/Login", "c6")],
            "main",
        );
        app.apply(Action::OpenPicker, 10);
        for character in "login".chars() {
            app.apply(Action::PickerChar(character), 10);
        }
        assert_eq!(app.picker.as_ref().expect("picker").visible().len(), 1);
        app.apply(Action::PickerConfirm, 10);
        assert!(app.picker.is_none());
        assert_eq!(app.selected, 6);
    }

    #[test]
    fn back_closes_the_picker_then_leaves_the_detail_panel() {
        let mut app = app_with(3, vec![], "main");
        app.apply(Action::FocusNext, 10);
        app.apply(Action::OpenPicker, 10);
        app.apply(Action::Back, 10);
        assert!(app.picker.is_none());
        assert_eq!(app.focus, Focus::Detail);
        app.apply(Action::Back, 10);
        assert_eq!(app.focus, Focus::Graph);
    }

    #[test]
    fn jump_to_a_ref_not_loaded_yet_asks_for_more_and_lands_when_it_arrives() {
        let mut app = app_with(3, vec![reference("old-tag", "c40")], "main");
        app.apply(Action::OpenPicker, 10);
        assert_eq!(app.apply(Action::PickerConfirm, 10), Effect::NeedMore);
        let more = (3..50)
            .map(|index| commit(&format!("c{index}"), &[]))
            .collect();
        app.add_commits(more, 10);
        assert_eq!(app.selected, 40);
    }

    fn app_in_panel(files: usize) -> App {
        let mut app = app_with(3, vec![], "main");
        app.file_count = files;
        app.apply(Action::FocusNext, 10);
        app
    }

    #[test]
    fn detail_focus_moves_the_file_cursor_instead_of_the_selection() {
        let mut app = app_in_panel(2);
        app.apply(Action::Down, 10);
        app.apply(Action::Down, 10);
        assert_eq!((app.selected, app.file_cursor), (0, 1));
    }

    #[test]
    fn enter_on_the_graph_reviews_the_whole_commit_in_tuicr() {
        let mut app = app_with(3, vec![], "main");
        app.apply(Action::Down, 10);
        assert_eq!(
            app.apply(Action::Open, 10),
            Effect::Review {
                hash: "c1".to_string()
            }
        );
    }

    #[test]
    fn enter_in_the_panel_reviews_the_whole_commit_in_tuicr() {
        let mut app = app_in_panel(3);
        app.apply(Action::Down, 10);
        assert_eq!(
            app.apply(Action::Open, 10),
            Effect::Review {
                hash: "c0".to_string()
            }
        );
    }

    #[test]
    fn changing_commit_resets_the_file_cursor() {
        let mut app = app_in_panel(2);
        app.apply(Action::Down, 10);
        app.apply(Action::FocusNext, 10);
        app.apply(Action::Down, 10);
        assert_eq!((app.file_cursor, app.selected), (0, 1));
    }

    #[test]
    fn selected_row_grows_to_one_line_per_ref() {
        let mut app = app_with(
            3,
            vec![
                reference("a", "c0"),
                reference("b", "c0"),
                reference("c", "c0"),
                reference("d", "c1"),
            ],
            "main",
        );
        app.row_lines = 2;
        assert_eq!(
            (app.row_height(0), app.row_height(1), app.row_height(2)),
            (3, 2, 2)
        );
        app.apply(Action::Down, 10);
        assert_eq!((app.row_height(0), app.row_height(1)), (2, 2));
    }

    fn remote(name: &str, target: &str) -> Ref {
        Ref {
            name: name.to_string(),
            kind: RefKind::Remote {
                remote: "origin".to_string(),
            },
            target: target.to_string(),
        }
    }

    #[test]
    fn space_on_a_commit_with_one_branch_checks_it_out() {
        let mut app = app_with(3, vec![reference("feature", "c0")], "main");
        assert_eq!(
            app.apply(Action::Checkout, 10),
            Effect::Checkout {
                name: "feature".to_string()
            }
        );
    }

    #[test]
    fn space_on_a_commit_without_branch_explains_why_nothing_happens() {
        let mut app = app_with(
            3,
            vec![Ref {
                name: "v1".to_string(),
                kind: RefKind::Tag,
                target: "c0".to_string(),
            }],
            "main",
        );
        assert_eq!(app.apply(Action::Checkout, 10), Effect::None);
        assert_eq!(app.status, "no branch on this commit");
    }

    #[test]
    fn space_on_a_commit_with_several_branches_lets_you_pick_one() {
        let mut app = app_with(
            3,
            vec![
                reference("staging", "c0"),
                remote("origin/staging", "c0"),
                reference("other", "c2"),
            ],
            "main",
        );
        app.apply(Action::Checkout, 10);
        let names: Vec<String> = app
            .picker
            .as_ref()
            .expect("picker")
            .visible()
            .iter()
            .map(|(entry, _)| entry.label.clone())
            .collect();
        assert_eq!(
            names,
            vec!["staging".to_string(), "origin/staging".to_string()]
        );
        app.apply(Action::PickerDown, 10);
        assert_eq!(
            app.apply(Action::PickerConfirm, 10),
            Effect::Checkout {
                name: "origin/staging".to_string()
            }
        );
    }

    #[test]
    fn confirmation_runs_on_yes_and_cancels_otherwise() {
        let mut app = app_with(3, vec![], "main");
        app.confirm = Some(Confirmation {
            message: "Reset?".to_string(),
            action: PendingAction::Checkout("origin/staging".to_string()),
        });
        assert_eq!(app.apply(Action::Confirm(false), 10), Effect::None);
        assert!(app.confirm.is_none());
        app.confirm = Some(Confirmation {
            message: "Reset?".to_string(),
            action: PendingAction::Checkout("origin/staging".to_string()),
        });
        assert_eq!(
            app.apply(Action::Confirm(true), 10),
            Effect::Confirmed(PendingAction::Checkout("origin/staging".to_string()))
        );
    }

    #[test]
    fn new_refs_replace_the_old_labels() {
        let mut app = app_with(3, vec![reference("main", "c0")], "main");
        app.set_refs(Refs {
            refs: vec![reference("feature", "c1")],
            head: Head::Branch("feature".to_string()),
        });
        assert!(app.refs_at(0).is_empty());
        assert_eq!(app.refs_at(1).len(), 1);
    }

    fn visible_labels(app: &App) -> Vec<String> {
        app.picker
            .as_ref()
            .expect("picker")
            .visible()
            .iter()
            .map(|(entry, _)| entry.label.clone())
            .collect()
    }

    #[test]
    fn slash_finds_commits_branches_and_tags_together() {
        let mut app = app_with(5, vec![reference("feature", "c1")], "main");
        app.apply(Action::OpenFinder, 10);
        let labels = visible_labels(&app);
        assert!(labels.contains(&"feature".to_string()), "{labels:?}");
        assert!(labels.contains(&"subject c3".to_string()), "{labels:?}");
    }

    #[test]
    fn finder_jumps_to_the_commit_it_found() {
        let mut app = app_with(5, vec![], "main");
        app.apply(Action::OpenFinder, 10);
        for character in "subject c3".chars() {
            app.apply(Action::PickerChar(character), 10);
        }
        assert_eq!(visible_labels(&app)[0], "subject c3");
        app.apply(Action::PickerConfirm, 10);
        assert_eq!(app.selected, 3);
    }

    #[test]
    fn branch_list_does_not_offer_commits() {
        let mut app = app_with(5, vec![reference("feature", "c1")], "main");
        app.apply(Action::OpenPicker, 10);
        assert_eq!(visible_labels(&app), vec!["feature".to_string()]);
    }

    #[test]
    fn shift_enter_in_the_finder_checks_out_the_branch() {
        let mut app = app_with(5, vec![reference("feature", "c1")], "main");
        app.apply(Action::OpenFinder, 10);
        for character in "feature".chars() {
            app.apply(Action::PickerChar(character), 10);
        }
        assert_eq!(
            app.apply(Action::PickerCheckout, 10),
            Effect::Checkout {
                name: "feature".to_string()
            }
        );
        assert!(app.picker.is_none());
    }

    #[test]
    fn shift_enter_on_a_commit_explains_it_needs_a_branch() {
        let mut app = app_with(5, vec![], "main");
        app.apply(Action::OpenFinder, 10);
        for character in "subject c3".chars() {
            app.apply(Action::PickerChar(character), 10);
        }
        assert_eq!(app.apply(Action::PickerCheckout, 10), Effect::None);
        assert_eq!(app.status, "pick a branch to check out");
        assert!(app.picker.is_some());
    }

    #[test]
    fn question_mark_toggles_the_help_and_any_closing_key_leaves_it() {
        let mut app = app_with(3, vec![], "main");
        app.apply(Action::ToggleHelp, 10);
        assert!(app.help_open);
        app.apply(Action::Down, 10);
        assert_eq!((app.help_open, app.selected, app.help_scroll), (true, 0, 1));
        app.apply(Action::Back, 10);
        assert!(!app.help_open);
        app.apply(Action::ToggleHelp, 10);
        app.apply(Action::ToggleHelp, 10);
        assert!(!app.help_open);
    }

    #[test]
    fn after_a_checkout_the_selection_follows_the_new_head() {
        let mut app = app_with(20, vec![reference("main", "c0")], "main");
        app.set_refs(Refs {
            refs: vec![reference("feature", "c15")],
            head: Head::Branch("feature".to_string()),
        });
        assert_eq!(app.focus_head(10), Effect::LoadDetail("c15".to_string()));
        assert_eq!((app.selected, app.offset), (15, 6));
    }

    #[test]
    fn capital_r_offers_the_three_reset_modes_and_requests_the_chosen_one() {
        let mut app = app_with(5, vec![], "main");
        app.apply(Action::Down, 10);
        app.apply(Action::ResetMenu, 10);
        assert_eq!(visible_labels(&app).len(), 3);
        app.apply(Action::PickerDown, 10);
        app.apply(Action::PickerDown, 10);
        assert_eq!(
            app.apply(Action::PickerConfirm, 10),
            Effect::Request(PendingAction::Reset {
                mode: ResetMode::Hard,
                hash: "c1".to_string()
            })
        );
    }

    #[test]
    fn small_r_rebases_onto_the_commit_branch_or_its_hash() {
        let mut app = app_with(5, vec![reference("feature", "c2")], "main");
        app.apply(Action::Down, 10);
        assert_eq!(
            app.apply(Action::Rebase, 10),
            Effect::Request(PendingAction::Rebase {
                onto: "c1".to_string()
            })
        );
        app.apply(Action::Down, 10);
        assert_eq!(
            app.apply(Action::Rebase, 10),
            Effect::Request(PendingAction::Rebase {
                onto: "feature".to_string()
            })
        );
    }

    #[test]
    fn x_aborts_only_while_a_rebase_is_in_progress() {
        let mut app = app_with(3, vec![], "main");
        assert_eq!(app.apply(Action::AbortRebase, 10), Effect::None);
        app.rebasing = true;
        assert_eq!(app.apply(Action::AbortRebase, 10), Effect::AbortRebase);
    }

    #[test]
    fn clearing_the_history_starts_the_layout_over_and_refocuses_head_once_reloaded() {
        let mut app = app_with(5, vec![reference("main", "c0")], "main");
        app.apply(Action::Down, 10);
        app.clear_history();
        assert_eq!(
            (
                app.commits.len(),
                app.rows.len(),
                app.selected,
                app.loading_done
            ),
            (0, 0, 0, false)
        );
        app.set_refs(Refs {
            refs: vec![reference("main", "n1")],
            head: Head::Branch("main".to_string()),
        });
        assert_eq!(app.focus_head(10), Effect::NeedMore);
        app.add_commits(vec![commit("n0", &["n1"]), commit("n1", &[])], 10);
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn a_refresh_keeps_the_selection_on_the_same_commit() {
        let mut app = app_with(10, vec![reference("main", "c0")], "main");
        for _ in 0..4 {
            app.apply(Action::Down, 10);
        }
        let hash = app.commits[app.selected].hash.clone();
        app.clear_history();
        app.restore_selection(hash, 10);
        app.add_commits(
            (0..10)
                .map(|index| commit(&format!("c{index}"), &[]))
                .collect(),
            10,
        );
        assert_eq!(app.selected, 4);
    }

    #[test]
    fn a_refresh_falls_back_to_head_when_the_commit_is_gone() {
        let mut app = app_with(5, vec![reference("main", "c2")], "main");
        app.clear_history();
        app.restore_selection("vanished".to_string(), 10);
        app.add_commits(
            (0..5)
                .map(|index| commit(&format!("c{index}"), &[]))
                .collect(),
            10,
        );
        app.finish_loading();
        app.resolve_pending(10);
        assert_eq!((app.selected, app.status.as_str()), (2, ""));
    }

    #[test]
    fn f_requests_a_fetch() {
        let mut app = app_with(3, vec![], "main");
        assert_eq!(app.apply(Action::Fetch, 10), Effect::Fetch);
    }

    #[test]
    fn refs_at_returns_every_ref_on_a_commit() {
        let app = app_with(
            3,
            vec![reference("main", "c1"), reference("origin/main", "c1")],
            "main",
        );
        assert_eq!(app.refs_at(1).len(), 2);
        assert!(app.refs_at(2).is_empty());
    }
}
