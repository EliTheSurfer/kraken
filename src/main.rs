use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};
use std::thread;
use std::time::Duration;

use anyhow::Result;
use image::{DynamicImage, RgbaImage};
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{
    self, Event, KeyEventKind, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use ratatui::crossterm::execute;
use ratatui::layout::Size;
use ratatui_image::Resize;
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::protocol::Protocol;

use kraken::avatars::cache::Cache;
use kraken::avatars::resolve::Forge;
use kraken::avatars::{AvatarStore, author_color};
use kraken::detail_loader::DetailLoader;
use kraken::git::{
    CheckoutPlan, Commit, CommitStream, Head, RebaseOutcome, RefKind, Repo, abort_rebase,
    changed_files, commits_dropped_by_reset, commits_leaving, plan_checkout, read_refs,
    rebase_in_progress, rebase_onto, reset_mode_question, reset_question, reset_to, run_checkout,
};
use kraken::render::pixels::{Geometry, NodeLook, draw_window, visible_lanes};
use kraken::sync::{SyncCommand, SyncEvent, SyncWorker};
use kraken::ui::keys::{InputMode, action_for};
use kraken::ui::state::{App, Confirmation, Effect, PendingAction};
use kraken::ui::view::{GraphWidget, ViewContext, draw, graph_area, panel_fits, visible_rows};

const BATCH_SIZE: usize = 500;
const SYNC_POLL: Duration = Duration::from_secs(2);
const FETCH_INTERVAL: Duration = Duration::from_secs(60);
const TALL_ROW_LINES: u16 = 2;
const GITHUB_ICON: &str = "\u{f09b}";
const GITLAB_ICON: &str = "\u{f296}";
const GIT_ICON: &str = "\u{f1d3}";

enum Loaded {
    Batch(Vec<Commit>),
    Done,
    Failed(String),
}

struct Options {
    path: PathBuf,
    text_only: bool,
}

fn parse_options() -> Options {
    let mut options = Options {
        path: PathBuf::from("."),
        text_only: false,
    };
    for argument in std::env::args().skip(1) {
        match argument.as_str() {
            "--text" => options.text_only = true,
            path => options.path = PathBuf::from(path),
        }
    }
    options
}

fn main() {
    let options = parse_options();
    let repo = match Repo::discover(&options.path) {
        Ok(repo) => repo,
        Err(error) => {
            eprintln!("kraken: {error:#}");
            std::process::exit(1);
        }
    };
    if let Err(error) = run(repo, options) {
        eprintln!("kraken: {error:#}");
        std::process::exit(1);
    }
}

fn run(repo: Repo, options: Options) -> Result<()> {
    let refs = read_refs(&repo)?;
    let remote_url = repo
        .run(&["remote", "get-url", "origin"])
        .unwrap_or_default();
    let remote_icon = remote_icon(&remote_url);
    let forge = forge_for(&remote_url);
    let picker = if options.text_only {
        None
    } else {
        Picker::from_query_stdio().ok()
    };
    let picker = picker.filter(|picker| picker.protocol_type() != ProtocolType::Halfblocks);
    let repo_name = repo
        .root
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();

    let loaded = spawn_loader(&repo)?;
    let avatars = picker.as_ref().map(|_| {
        AvatarStore::start(
            repo.root.clone(),
            forge,
            Cache {
                dir: Cache::default_dir(),
            },
        )
    });

    let mut terminal = open_terminal();
    let result = event_loop(
        &mut terminal,
        repo,
        refs,
        loaded,
        picker,
        avatars,
        &remote_icon,
        &repo_name,
    );
    close_terminal();
    result
}

fn open_terminal() -> DefaultTerminal {
    let terminal = ratatui::init();
    let _ = execute!(
        std::io::stdout(),
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
    );
    terminal
}

fn close_terminal() {
    let _ = execute!(std::io::stdout(), PopKeyboardEnhancementFlags);
    ratatui::restore();
}

fn remote_icon(remote_url: &str) -> String {
    if remote_url.contains("github") {
        return GITHUB_ICON.to_string();
    }
    if remote_url.contains("gitlab") {
        return GITLAB_ICON.to_string();
    }
    GIT_ICON.to_string()
}

fn forge_for(remote_url: &str) -> Forge {
    if let Some((owner, repo)) = github_repository(remote_url) {
        return Forge::Github { owner, repo };
    }
    if remote_url.contains("gitlab") {
        return Forge::Gitlab;
    }
    Forge::Other
}

fn github_repository(remote_url: &str) -> Option<(String, String)> {
    let path = remote_url.trim().split("github.com").nth(1)?;
    let path = path.trim_start_matches([':', '/']).trim_end_matches(".git");
    let (owner, name) = path.split_once('/')?;
    Some((owner.to_string(), name.to_string()))
}

fn spawn_loader(repo: &Repo) -> Result<Receiver<Loaded>> {
    let mut stream = CommitStream::spawn(repo)?;
    let (sender, receiver) = channel();
    thread::spawn(move || {
        loop {
            match stream.next_batch(BATCH_SIZE) {
                Ok(batch) if batch.is_empty() => {
                    let _ = sender.send(Loaded::Done);
                    return;
                }
                Ok(batch) => {
                    if sender.send(Loaded::Batch(batch)).is_err() {
                        return;
                    }
                }
                Err(error) => {
                    let _ = sender.send(Loaded::Failed(format!("{error:#}")));
                    return;
                }
            }
        }
    });
    Ok(receiver)
}

#[derive(PartialEq)]
struct GraphKey {
    offset: usize,
    lines: Vec<u16>,
    lanes: usize,
    avatars: usize,
}

#[allow(clippy::too_many_arguments)]
fn event_loop(
    terminal: &mut DefaultTerminal,
    repo: Repo,
    refs: kraken::git::Refs,
    mut loaded: Receiver<Loaded>,
    picker: Option<Picker>,
    mut avatars: Option<AvatarStore>,
    remote_icon: &str,
    repo_name: &str,
) -> Result<()> {
    let mut app = App::new(refs);
    app.rebasing = rebase_in_progress(&repo);
    let sync = SyncWorker::start(repo.root.clone(), SYNC_POLL, FETCH_INTERVAL);
    let geometry = picker.as_ref().map(|picker| {
        let font = picker.font_size();
        Geometry::for_cell(
            font.width as f32,
            font.height as f32 * TALL_ROW_LINES as f32,
        )
    });
    if let Some(geometry) = geometry {
        app.lane_cells = geometry.lane_cells as u16;
        app.row_lines = TALL_ROW_LINES;
    }
    let mut details = DetailLoader::start(repo.root.clone());
    let mut avatar_images: HashMap<String, RgbaImage> = HashMap::new();
    let mut graph: Option<(GraphKey, Protocol)> = None;

    loop {
        let size = terminal.size()?;
        let page = (size.height.saturating_sub(3) / app.row_lines) as usize;
        app.panel_visible = panel_fits(size.width);
        for event in sync.events.try_iter() {
            match event {
                SyncEvent::Fetching => app.sync_note = "fetching…".to_string(),
                SyncEvent::Fetched { error: None } => {
                    app.sync_note = format!("fetched {}", chrono::Local::now().format("%H:%M"));
                }
                SyncEvent::Fetched { error: Some(error) } => {
                    app.sync_note = format!("fetch failed: {error}")
                }
                SyncEvent::Changed => {
                    loaded = refresh(&repo, &mut app, page)?;
                    graph = None;
                }
            }
        }
        for message in loaded.try_iter() {
            match message {
                Loaded::Batch(batch) => {
                    app.add_commits(batch, page);
                }
                Loaded::Done => {
                    app.finish_loading();
                    app.resolve_pending(page);
                }
                Loaded::Failed(error) => {
                    app.finish_loading();
                    app.status = error;
                }
            }
        }
        let wanted = app
            .commits
            .get(app.selected)
            .map(|commit| commit.hash.clone());
        if let Some(hash) = &wanted {
            details.want(hash);
        }
        for (hash, error) in details.poll() {
            if Some(&hash) == wanted.as_ref() {
                app.status = error;
            }
        }
        let selected_detail = wanted.as_ref().and_then(|hash| details.get(hash));
        app.file_count = selected_detail.map_or(0, |detail| detail.files.len());
        if let Some(store) = avatars.as_mut() {
            for (email, image) in store.poll() {
                avatar_images.insert(email, image);
            }
            for commit in app.commits.iter().skip(app.offset).take(page) {
                store.request(&commit.author.email, &commit.author.name, &commit.hash);
            }
        }

        let area = terminal.size()?;
        let full = ratatui::layout::Rect::new(0, 0, area.width, area.height);
        if let (Some(picker), Some(geometry)) = (&picker, geometry) {
            let target = graph_area(&app, full);
            let end = (app.offset + visible_rows(&app, target.height)).min(app.rows.len());
            let window = &app.rows[app.offset.min(end)..end];
            let lines: Vec<u16> = (app.offset..end)
                .map(|index| app.row_height(index))
                .collect();
            let key = GraphKey {
                offset: app.offset,
                lines: lines.clone(),
                lanes: visible_lanes(window, (target.width / app.lane_cells) as usize),
                avatars: avatar_images.len(),
            };
            if graph.as_ref().is_none_or(|(current, _)| *current != key) && !window.is_empty() {
                let nodes: Vec<NodeLook> = app.commits[app.offset..end]
                    .iter()
                    .map(|commit| NodeLook {
                        avatar: avatar_images.get(&commit.author.email.trim().to_lowercase()),
                        author_color: author_color(&commit.author.email),
                    })
                    .collect();
                let line_height = geometry.cell_h / app.row_lines as f32;
                let heights: Vec<f32> = lines
                    .iter()
                    .map(|count| *count as f32 * line_height)
                    .collect();
                let image = draw_window(window, &nodes, key.lanes, geometry, &heights);
                let size = Size::new((key.lanes * geometry.lane_cells) as u16, lines.iter().sum());
                let protocol = picker.new_protocol(
                    DynamicImage::ImageRgba8(image),
                    size,
                    Resize::Fit(None),
                )?;
                graph = Some((key, protocol));
            }
        }

        let selected_hash = app
            .commits
            .get(app.selected)
            .map(|commit| commit.hash.clone());
        terminal.draw(|frame| {
            let graph_widget = match &graph {
                Some((_, protocol)) => GraphWidget::Pixels(protocol),
                None => GraphWidget::Text,
            };
            let context = ViewContext {
                graph: graph_widget,
                detail: selected_hash.as_ref().and_then(|hash| details.get(hash)),
                remote_icon,
                repo_name,
            };
            draw(frame, &app, &context);
        })?;

        if !event::poll(Duration::from_millis(50))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        let mode = if app.confirm.is_some() {
            InputMode::Confirm
        } else if app.picker.is_some() {
            InputMode::Picker
        } else {
            InputMode::Normal
        };
        let Some(action) = action_for(key, mode) else {
            continue;
        };
        match app.apply(action, page) {
            Effect::Quit => return Ok(()),
            Effect::Review { hash } => {
                if let Err(message) = review_in_tuicr(terminal, &repo.root, &hash) {
                    app.status = message;
                }
                graph = None;
            }
            Effect::Checkout { name } => {
                checkout(&repo, &mut app, &name, false, page);
                let _ = sync.commands.send(SyncCommand::Rebaseline);
            }
            Effect::Confirmed(PendingAction::Checkout(name)) => {
                checkout(&repo, &mut app, &name, true, page);
                let _ = sync.commands.send(SyncCommand::Rebaseline);
            }
            Effect::Fetch => {
                app.sync_note = "fetching…".to_string();
                let _ = sync.commands.send(SyncCommand::FetchNow);
            }
            Effect::Request(action) => ask_confirmation(&repo, &mut app, action),
            Effect::Confirmed(action) => {
                app.status = match rewrite_history(&repo, &mut app, &action) {
                    Ok(message) => message,
                    Err(error) => first_line(&format!("{error:#}")),
                };
                loaded = reload(&repo, &mut app, page)?;
                let _ = sync.commands.send(SyncCommand::Rebaseline);
                graph = None;
            }
            Effect::AbortRebase => {
                app.status = match abort_rebase(&repo) {
                    Ok(()) => "rebase aborted".to_string(),
                    Err(error) => first_line(&format!("{error:#}")),
                };
                loaded = reload(&repo, &mut app, page)?;
                let _ = sync.commands.send(SyncCommand::Rebaseline);
                graph = None;
            }
            _ => {}
        }
    }
}

fn current_branch_label(app: &App) -> String {
    match &app.refs.head {
        Head::Branch(name) => name.clone(),
        Head::Detached(_) => "HEAD (detached)".to_string(),
    }
}

fn short(target: &str) -> String {
    let is_hash = target.len() == 40
        && target
            .chars()
            .all(|character| character.is_ascii_hexdigit());
    if is_hash {
        target.chars().take(7).collect()
    } else {
        target.to_string()
    }
}

fn ask_confirmation(repo: &Repo, app: &mut App, action: PendingAction) {
    let branch = current_branch_label(app);
    let message = match &action {
        PendingAction::Reset { mode, hash } => {
            let counts =
                commits_leaving(repo, hash).and_then(|leaving| Ok((leaving, changed_files(repo)?)));
            match counts {
                Ok((leaving, changed)) => {
                    reset_mode_question(*mode, &branch, &short(hash), leaving, changed)
                }
                Err(error) => {
                    app.status = first_line(&format!("{error:#}"));
                    return;
                }
            }
        }
        PendingAction::Rebase { onto } => format!("Rebase {branch} onto {}?", short(onto)),
        PendingAction::Checkout(_) => return,
    };
    app.confirm = Some(Confirmation { message, action });
}

fn rewrite_history(repo: &Repo, app: &mut App, action: &PendingAction) -> Result<String> {
    match action {
        PendingAction::Reset { mode, hash } => reset_to(repo, *mode, hash),
        PendingAction::Rebase { onto } => match rebase_onto(repo, onto)? {
            RebaseOutcome::Done => Ok(format!("rebased onto {}", short(onto))),
            RebaseOutcome::Conflict => {
                app.rebasing = true;
                Ok("rebase stopped on conflicts".to_string())
            }
        },
        PendingAction::Checkout(_) => Ok(String::new()),
    }
}

fn refresh(repo: &Repo, app: &mut App, page: usize) -> Result<Receiver<Loaded>> {
    let selected = app
        .commits
        .get(app.selected)
        .map(|commit| commit.hash.clone());
    app.rebasing = rebase_in_progress(repo);
    if let Ok(refs) = read_refs(repo) {
        app.set_refs(refs);
    }
    app.clear_history();
    let loaded = spawn_loader(repo)?;
    match selected {
        Some(hash) => app.restore_selection(hash, page),
        None => app.focus_head(page),
    };
    Ok(loaded)
}

fn reload(repo: &Repo, app: &mut App, page: usize) -> Result<Receiver<Loaded>> {
    app.rebasing = rebase_in_progress(repo);
    if let Ok(refs) = read_refs(repo) {
        app.set_refs(refs);
    }
    app.clear_history();
    let loaded = spawn_loader(repo)?;
    app.focus_head(page);
    Ok(loaded)
}

fn checkout(repo: &Repo, app: &mut App, name: &str, confirmed: bool, page: usize) {
    let Some(chosen) = app
        .refs
        .refs
        .iter()
        .find(|reference| reference.name == name && reference.kind != RefKind::Tag)
        .cloned()
    else {
        app.status = format!("{name} is not a branch");
        return;
    };
    let plan = plan_checkout(&app.refs, &chosen);
    if let (CheckoutPlan::Reset { local, remote, .. }, false) = (&plan, confirmed) {
        match commits_dropped_by_reset(repo, local, remote) {
            Ok(dropped) => {
                app.confirm = Some(Confirmation {
                    message: reset_question(local, remote, dropped),
                    action: PendingAction::Checkout(name.to_string()),
                });
            }
            Err(error) => app.status = first_line(&format!("{error:#}")),
        }
        return;
    }
    let succeeded = match run_checkout(repo, &plan) {
        Ok(message) => {
            app.status = message;
            true
        }
        Err(error) => {
            app.status = first_line(&format!("{error:#}"));
            false
        }
    };
    match read_refs(repo) {
        Ok(refs) => app.set_refs(refs),
        Err(error) => app.status = first_line(&format!("{error:#}")),
    }
    if succeeded {
        app.focus_head(page);
    }
}

fn first_line(text: &str) -> String {
    text.lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or(text)
        .to_string()
}

fn tuicr_arguments(hash: &str) -> Vec<String> {
    vec![
        "--no-update-check".to_string(),
        "-r".to_string(),
        hash.to_string(),
    ]
}

fn review_in_tuicr(
    terminal: &mut DefaultTerminal,
    repo_root: &std::path::Path,
    hash: &str,
) -> Result<(), String> {
    close_terminal();
    let status = std::process::Command::new("tuicr")
        .args(tuicr_arguments(hash))
        .current_dir(repo_root)
        .status();
    *terminal = open_terminal();
    let _ = terminal.clear();
    match status {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Err("tuicr is not installed: see https://github.com/agavra/tuicr".to_string())
        }
        Err(error) => Err(format!("cannot run tuicr: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_repository_is_read_from_ssh_and_https_remotes() {
        assert_eq!(
            github_repository("git@github.com:octocat/hello.git"),
            Some(("octocat".into(), "hello".into()))
        );
        assert_eq!(
            github_repository("https://github.com/example-org/example-app.git\n"),
            Some(("example-org".into(), "example-app".into()))
        );
        assert_eq!(
            github_repository("git@gitlab.example.net:team/app.git"),
            None
        );
    }

    #[test]
    fn tuicr_reviews_the_whole_commit() {
        assert_eq!(
            tuicr_arguments("abc"),
            vec!["--no-update-check", "-r", "abc"]
        );
    }

    #[test]
    fn forge_follows_the_origin_host() {
        assert_eq!(
            forge_for("git@github.com:octocat/hello.git"),
            Forge::Github {
                owner: "octocat".into(),
                repo: "hello".into()
            }
        );
        assert_eq!(
            forge_for("git@gitlab.example.net:team/app.git"),
            Forge::Gitlab
        );
        assert_eq!(forge_for("https://bitbucket.org/a/b.git"), Forge::Other);
    }

    #[test]
    fn remote_icon_follows_the_host() {
        assert_eq!(remote_icon("git@github.com:a/b.git"), GITHUB_ICON);
        assert_eq!(remote_icon("git@gitlab.example.net:a/b.git"), GITLAB_ICON);
        assert_eq!(remote_icon(""), GIT_ICON);
    }
}
