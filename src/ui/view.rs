use chrono::{Local, TimeZone};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui_image::Image;
use ratatui_image::protocol::Protocol;

use super::finder::EntryKind;
use super::state::{App, Focus, PickerPurpose};
use crate::git::{Detail, Head, Ref, RefBadge, RefKind, ref_badges};
use crate::render::palette::rgb;
use crate::render::text::{render_continuation, render_row};

pub enum GraphWidget<'a> {
    Pixels(&'a Protocol),
    Text,
}

pub struct ViewContext<'a> {
    pub graph: GraphWidget<'a>,
    pub detail: Option<&'a Detail>,
    pub remote_icon: &'a str,
    pub ref_logos: Option<&'a RefLogos>,
    pub repo_name: &'a str,
}

pub struct RefLogos {
    pub local: Vec<Protocol>,
    pub remote: Vec<Protocol>,
}

impl RefLogos {
    fn get(&self, mark: Mark, color: usize) -> Option<&Protocol> {
        let logos = match mark {
            Mark::Local => &self.local,
            Mark::Remote => &self.remote,
        };
        logos.get(color % logos.len().max(1))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Mark {
    Local,
    Remote,
}

struct RefLabel {
    line: Line<'static>,
    marks: Vec<(u16, Mark)>,
}

pub const LOGO_CELLS: u16 = 2;
pub const LABEL_INK: (u8, u8, u8) = (16, 18, 26);
pub const GRAPH_MAX_SHARE: f32 = 0.4;
pub const DETAIL_MIN_WIDTH: u16 = 120;
const REF_COLUMN_MIN: u16 = 22;
const REF_COLUMN_MAX: u16 = 50;
const REF_COLUMN_MAX_SHARE: f32 = 0.3;
const REF_LABEL_DECORATION: usize = 9;
const GRAPH_MIN_SHARE: f32 = 0.18;
const LOCAL_ICON: &str = "\u{f109}";
const DATE_COLUMN: u16 = 6;
const GRAPH_TITLE_WIDTH: u16 = 6;
const TAG_ICON: &str = "\u{f02b}";
const HEADER: Color = Color::Rgb(130, 135, 150);
const SELECTED: Color = Color::Rgb(38, 64, 120);
const HEAD_ROW: Color = Color::Rgb(30, 64, 44);
const MATCH: Color = Color::Rgb(238, 146, 60);
const COMMIT_ICON: &str = "\u{f417}";
const BRANCH_ICON: &str = "\u{e725}";
const RESET_ICON: &str = "\u{f0e2}";
const PANEL_TEXT: Color = Color::Rgb(200, 204, 214);

pub fn graph_columns(app: &App, rows_visible: usize, main_width: u16) -> u16 {
    let end = (app.offset + rows_visible).min(app.rows.len());
    let lanes = app.rows[app.offset.min(end)..end]
        .iter()
        .map(|row| row.width)
        .max()
        .unwrap_or(1)
        .max(1) as u16;
    let lane_cells = app.lane_cells.max(1);
    let cap = ((main_width as f32 * GRAPH_MAX_SHARE) as u16 / lane_cells).max(1);
    let min_width = (main_width as f32 * GRAPH_MIN_SHARE) as u16;
    (lanes.min(cap) * lane_cells).max(min_width)
}

pub fn ref_column(app: &App, main_width: u16) -> u16 {
    let longest = app
        .refs
        .refs
        .iter()
        .map(|reference| reference.short_name().chars().count() + REF_LABEL_DECORATION)
        .max()
        .unwrap_or(0);
    let cap =
        ((main_width as f32 * REF_COLUMN_MAX_SHARE) as u16).clamp(REF_COLUMN_MIN, REF_COLUMN_MAX);
    u16::try_from(longest)
        .unwrap_or(u16::MAX)
        .clamp(REF_COLUMN_MIN, cap)
}

pub fn areas(app: &App, full: Rect) -> (Rect, Rect, Option<Rect>, Rect) {
    let [header, body, status] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(2),
    ])
    .areas(full);
    let show_panel = app.show_detail && panel_fits(full.width);
    let (main, panel) = if show_panel {
        let [main, panel] = Layout::horizontal([
            Constraint::Percentage(100 - PANEL_SHARE),
            Constraint::Percentage(PANEL_SHARE),
        ])
        .areas(body);
        (main, Some(panel))
    } else {
        (body, None)
    };
    (header, main, panel, status)
}

const PANEL_SHARE: u16 = 25;

pub fn panel_fits(width: u16) -> bool {
    width >= DETAIL_MIN_WIDTH
}

pub fn visible_rows(app: &App, height: u16) -> usize {
    let mut used = 0u16;
    let mut count = 0;
    for index in app.offset..app.commits.len() {
        used += app.row_height(index);
        if used > height {
            break;
        }
        count += 1;
    }
    count
}

pub fn graph_area(app: &App, full: Rect) -> Rect {
    let (_, main, _, _) = areas(app, full);
    let width = graph_columns(app, visible_rows(app, main.height), main.width);
    Rect {
        x: main.x + ref_column(app, main.width),
        y: main.y,
        width,
        height: main.height,
    }
}

pub fn draw(frame: &mut Frame, app: &App, context: &ViewContext) {
    let (header, main, panel, status) = areas(app, frame.area());
    let graph_width =
        graph_columns(app, visible_rows(app, main.height), main.width).max(GRAPH_TITLE_WIDTH);
    let [refs_area, graph, message_area, date_area] = Layout::horizontal([
        Constraint::Length(ref_column(app, main.width)),
        Constraint::Length(graph_width),
        Constraint::Min(10),
        Constraint::Length(DATE_COLUMN),
    ])
    .areas(main);
    draw_header(frame, header, [refs_area, graph, message_area, date_area]);
    draw_rows(
        frame,
        app,
        context,
        main,
        [refs_area, graph, message_area, date_area],
    );
    if let Some(panel) = panel {
        draw_panel(frame, app, context.detail, panel);
    }
    draw_status(
        frame,
        app,
        context,
        Rect {
            height: 1,
            ..status
        },
    );
    draw_hints(
        frame,
        app,
        Rect {
            y: status.y + 1,
            height: 1,
            ..status
        },
    );
    if app.picker.is_some() {
        draw_picker(frame, app, frame.area(), context.remote_icon);
    }
    if app.help_open {
        draw_help(frame, frame.area(), app.help_scroll);
    }
    if let Some(confirmation) = &app.confirm {
        draw_confirmation(frame, &confirmation.message, frame.area());
    }
}

fn draw_confirmation(frame: &mut Frame, message: &str, full: Rect) {
    let width = (message.chars().count() as u16 + 6).clamp(30, full.width);
    let area = Rect {
        x: full.x + (full.width - width) / 2,
        y: full.y + full.height.saturating_sub(5) / 2,
        width,
        height: 5.min(full.height),
    };
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Confirm ")
        .border_style(Style::default().fg(Color::Rgb(232, 200, 72)));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let lines = vec![
        Line::from(Span::styled(
            format!(" {message}"),
            Style::default().fg(Color::White),
        )),
        Line::from(Span::styled(" [y/N]", Style::default().fg(HEADER))),
    ];
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn draw_header(frame: &mut Frame, header: Rect, columns: [Rect; 4]) {
    let style = Style::default().fg(HEADER).add_modifier(Modifier::BOLD);
    for (column, title) in columns
        .iter()
        .zip(["BRANCH / TAG", "GRAPH", "COMMIT MESSAGE", "DATE"])
    {
        let area = Rect {
            y: header.y,
            height: 1,
            ..*column
        };
        frame.render_widget(Paragraph::new(Span::styled(title, style)), area);
    }
}

fn draw_rows(frame: &mut Frame, app: &App, context: &ViewContext, main: Rect, columns: [Rect; 4]) {
    let [refs_area, graph, message_area, date_area] = columns;
    let visible = visible_rows(app, main.height);
    let tall = app.row_lines > 1;
    let head = app.head_hash();
    let mut y = main.y;
    for index in app.offset..app.offset + visible {
        let Some(commit) = app.commits.get(index) else {
            break;
        };
        let height = app.row_height(index);
        if y + height > main.y + main.height {
            break;
        }
        let row_y = y;
        y += height;
        let y = row_y;
        let background = if index == app.selected {
            Some(SELECTED)
        } else if head.as_deref() == Some(commit.hash.as_str()) {
            Some(HEAD_ROW)
        } else {
            None
        };
        if let Some(background) = background {
            let highlight = Rect {
                x: main.x,
                y,
                width: main.width,
                height,
            };
            frame.render_widget(
                Block::default().style(Style::default().bg(background)),
                highlight,
            );
        }
        let row_style = background.map_or(Style::default(), |background| {
            Style::default().bg(background)
        });
        let color_index = app.rows[index].color;
        let lane_color = lane_color(color_index);
        for (line_offset, label) in ref_lines(
            app,
            index,
            height as usize,
            refs_area.width as usize,
            color_index,
            context,
        )
        .into_iter()
        .enumerate()
        {
            let label_area = cell(refs_area, y + line_offset as u16);
            frame.render_widget(Paragraph::new(label.line), label_area);
            draw_logos(frame, context, &label.marks, color_index, label_area);
        }
        if let GraphWidget::Text = context.graph {
            let text = render_row(&app.rows[index]);
            frame.render_widget(
                Paragraph::new(Span::styled(text, row_style.fg(lane_color))),
                cell(graph, y),
            );
            let continuation = render_continuation(&app.rows[index]);
            for line_offset in 1..height {
                frame.render_widget(
                    Paragraph::new(Span::styled(continuation.clone(), row_style.fg(lane_color))),
                    cell(graph, y + line_offset),
                );
            }
        }
        let message_style = if app.rows[index].merge {
            row_style.fg(HEADER)
        } else {
            row_style.fg(PANEL_TEXT)
        };
        frame.render_widget(
            Paragraph::new(Span::styled(format!(" {}", commit.subject), message_style)),
            cell(message_area, y),
        );
        frame.render_widget(
            Paragraph::new(Span::styled(
                short_date(commit.author.time),
                row_style.fg(HEADER),
            )),
            cell(date_area, y),
        );
        if tall {
            let hash: String = commit.hash.chars().take(7).collect();
            let byline = format!(" {} · {hash}", commit.author.name);
            frame.render_widget(
                Paragraph::new(Span::styled(byline, row_style.fg(HEADER))),
                cell(message_area, y + 1),
            );
            frame.render_widget(
                Paragraph::new(Span::styled(
                    short_time(commit.author.time),
                    row_style.fg(HEADER),
                )),
                cell(date_area, y + 1),
            );
        }
    }
    if let GraphWidget::Pixels(protocol) = context.graph {
        let image_width = graph_columns(app, visible, main.width);
        frame.render_widget(
            Image::new(protocol).allow_clipping(true),
            Rect {
                width: image_width.min(graph.width),
                ..graph
            },
        );
    }
}

fn cell(column: Rect, y: u16) -> Rect {
    Rect {
        y,
        height: 1,
        ..column
    }
}

fn lane_color(color: usize) -> Color {
    let (red, green, blue) = rgb(color);
    Color::Rgb(red, green, blue)
}

fn ordered_refs(app: &App, index: usize) -> Vec<&Ref> {
    let refs = app.refs_at(index);
    let Some(primary) = pick_primary(&refs, &app.refs.head) else {
        return refs;
    };
    let mut ordered = vec![primary];
    ordered.extend(
        refs.into_iter()
            .filter(|reference| !std::ptr::eq(*reference, primary)),
    );
    ordered
}

fn draw_logos(
    frame: &mut Frame,
    context: &ViewContext,
    marks: &[(u16, Mark)],
    color: usize,
    label_area: Rect,
) {
    let Some(logos) = context.ref_logos else {
        return;
    };
    for (offset, mark) in marks
        .iter()
        .filter(|(offset, _)| offset + LOGO_CELLS <= label_area.width)
    {
        if let Some(protocol) = logos.get(*mark, color) {
            frame.render_widget(
                Image::new(protocol),
                Rect {
                    x: label_area.x + offset,
                    width: LOGO_CELLS,
                    ..label_area
                },
            );
        }
    }
}

fn ref_lines(
    app: &App,
    index: usize,
    height: usize,
    width: usize,
    color: usize,
    context: &ViewContext,
) -> Vec<RefLabel> {
    let refs = ref_badges(&ordered_refs(app, index));
    let shown = if refs.len() > height {
        height.saturating_sub(1).max(1)
    } else {
        refs.len()
    };
    refs.iter()
        .take(shown)
        .enumerate()
        .map(|(position, reference)| {
            let hidden = refs.len() - shown;
            let extra = if position + 1 == shown && hidden > 0 {
                format!(" +{hidden}")
            } else {
                String::new()
            };
            ref_label(app, reference, &extra, width, color, context)
        })
        .collect()
}

fn ref_label(
    app: &App,
    badge: &RefBadge,
    extra: &str,
    width: usize,
    color_index: usize,
    context: &ViewContext,
) -> RefLabel {
    let color = lane_color(color_index);
    let checked_out =
        badge.local && matches!(&app.refs.head, Head::Branch(name) if name == &badge.name);
    let prefix = match (checked_out, badge.tag) {
        (true, _) => "✓ ".to_string(),
        (false, true) => format!("{TAG_ICON} "),
        (false, false) => String::new(),
    };
    let marks: Vec<Mark> = [(badge.local, Mark::Local), (badge.remote, Mark::Remote)]
        .into_iter()
        .filter(|(present, _)| *present)
        .map(|(_, mark)| mark)
        .collect();
    let mark_text = |mark: &Mark| match (context.ref_logos, mark) {
        (Some(_), _) => " ".repeat(LOGO_CELLS as usize),
        (None, Mark::Local) => LOCAL_ICON.to_string(),
        (None, Mark::Remote) => context.remote_icon.to_string(),
    };
    let suffix: String = marks
        .iter()
        .map(|mark| format!(" {}", mark_text(mark)))
        .chain([" ".to_string()])
        .collect();
    let room = width.saturating_sub(
        1 + extra.chars().count() + prefix.chars().count() + suffix.chars().count(),
    );
    let name = truncate(&badge.name, room);
    let marks_start = prefix.chars().count() + name.chars().count();
    let mark_offsets = marks
        .iter()
        .enumerate()
        .map(|(position, mark)| {
            let offset = marks_start + 1 + position * (LOGO_CELLS as usize + 1);
            (offset as u16, *mark)
        })
        .collect();
    RefLabel {
        line: Line::from(vec![
            Span::styled(
                format!("{prefix}{name}{suffix}"),
                Style::default().fg(lane_ink()).bg(color),
            ),
            Span::styled(extra.to_string(), Style::default().fg(color)),
        ]),
        marks: mark_offsets,
    }
}

fn lane_ink() -> Color {
    let (red, green, blue) = LABEL_INK;
    Color::Rgb(red, green, blue)
}

fn pick_primary<'a>(refs: &[&'a Ref], head: &Head) -> Option<&'a Ref> {
    let head_branch = refs.iter().find(|reference| matches!(head, Head::Branch(name) if name == &reference.name && reference.kind == RefKind::Local));
    head_branch
        .or_else(|| {
            refs.iter()
                .find(|reference| reference.kind == RefKind::Local)
        })
        .or_else(|| refs.first())
        .copied()
}

pub fn truncate(text: &str, room: usize) -> String {
    if text.chars().count() <= room {
        return text.to_string();
    }
    let kept: String = text.chars().take(room.saturating_sub(1)).collect();
    format!("{kept}…")
}

fn short_date(time: i64) -> String {
    Local
        .timestamp_opt(time, 0)
        .single()
        .map(|date| date.format("%m/%d").to_string())
        .unwrap_or_default()
}

fn short_time(time: i64) -> String {
    Local
        .timestamp_opt(time, 0)
        .single()
        .map(|date| date.format("%H:%M").to_string())
        .unwrap_or_default()
}

fn long_date(time: i64) -> String {
    Local
        .timestamp_opt(time, 0)
        .single()
        .map(|date| date.format("%m/%d/%Y @ %H:%M").to_string())
        .unwrap_or_default()
}

fn draw_panel(frame: &mut Frame, app: &App, detail: Option<&Detail>, area: Rect) {
    let block = panel_block(app);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let Some(commit) = app.commits.get(app.selected) else {
        return;
    };
    let mut lines = vec![Line::from(Span::styled(
        format!("commit {}", &commit.hash[..commit.hash.len().min(10)]),
        Style::default().fg(HEADER),
    ))];
    lines.push(Line::default());
    let message = detail.map_or(commit.subject.clone(), |detail| detail.message.clone());
    for (number, text) in message.lines().enumerate() {
        let style = if number == 0 {
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(PANEL_TEXT)
        };
        lines.push(Line::from(Span::styled(text.to_string(), style)));
    }
    lines.push(Line::default());
    lines.push(Line::from(vec![
        Span::styled(
            commit.author.name.clone(),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  authored {}", long_date(commit.author.time)),
            Style::default().fg(HEADER),
        ),
    ]));
    if commit.committer.email != commit.author.email {
        lines.push(Line::from(vec![
            Span::styled(
                commit.committer.name.clone(),
                Style::default().fg(PANEL_TEXT),
            ),
            Span::styled(
                format!("  committed {}", long_date(commit.committer.time)),
                Style::default().fg(HEADER),
            ),
        ]));
    }
    let parents: Vec<String> = commit
        .parents
        .iter()
        .map(|parent| parent.chars().take(7).collect())
        .collect();
    lines.push(Line::from(Span::styled(
        format!("parent: {}", parents.join(" ")),
        Style::default().fg(HEADER),
    )));
    lines.push(Line::default());
    if let Some(detail) = detail {
        lines.push(Line::from(Span::styled(
            format!("{} modified", detail.files.len()),
            Style::default().fg(Color::Rgb(232, 200, 72)),
        )));
        let room = (inner.height as usize).saturating_sub(lines.len()).max(1);
        let start = app.file_cursor.saturating_sub(room - 1);
        for (position, file) in detail.files.iter().enumerate().skip(start) {
            let highlighted = app.focus == Focus::Detail && position == app.file_cursor;
            let path_style = if highlighted {
                Style::default().fg(Color::White).bg(SELECTED)
            } else {
                Style::default().fg(PANEL_TEXT)
            };
            lines.push(Line::from(vec![
                Span::styled(
                    file.status.to_string(),
                    Style::default()
                        .fg(status_color(file.status))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(file.path.clone(), path_style),
            ]));
        }
    }
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }),
        Rect {
            x: inner.x + 1,
            width: inner.width.saturating_sub(1),
            ..inner
        },
    );
}

fn panel_block(app: &App) -> Block<'static> {
    let border = if app.focus == Focus::Detail {
        Color::Rgb(100, 140, 230)
    } else {
        Color::Rgb(60, 64, 80)
    };
    Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(border))
}

fn status_color(status: char) -> Color {
    match status {
        'A' => Color::Rgb(122, 196, 80),
        'D' => Color::Rgb(226, 72, 72),
        'R' => Color::Rgb(86, 182, 230),
        _ => Color::Rgb(232, 200, 72),
    }
}

pub fn hint_text(app: &App) -> &'static str {
    if app.help_open {
        return "j/k scroll · ?/esc/q close help";
    }
    if app.confirm.is_some() {
        return "y confirm · any other key cancels";
    }
    if let Some(picker) = &app.picker {
        return match picker.purpose {
            PickerPurpose::Checkout => "type to filter · ↑/↓ move · enter check out · esc cancel",
            PickerPurpose::Reset => "↑/↓ choose the mode · enter reset · esc cancel",
            PickerPurpose::Jump | PickerPurpose::Find => {
                "type to filter · ↑/↓ move · enter jump · shift-enter check out · esc close"
            }
        };
    }
    if app.focus == Focus::Detail {
        return "j/k files · enter review in tuicr · tab/esc back to graph · ? help";
    }
    "j/k move · / find · b branches · ]/[ ref · h HEAD · space check out · R reset · r rebase · f fetch · enter tuicr · tab files · ? help · q quit"
}

fn draw_hints(frame: &mut Frame, app: &App, area: Rect) {
    frame.render_widget(
        Paragraph::new(Span::styled(
            format!(" {}", hint_text(app)),
            Style::default().fg(HEADER),
        )),
        area,
    );
}

const HELP: &[(&str, &[(&str, &str)])] = &[
    (
        "Graph",
        &[
            ("j / k, ↓ / ↑", "next / previous commit"),
            ("g / G", "first / last commit"),
            ("ctrl-d / ctrl-u", "half page down / up"),
            ("] / [", "next / previous commit with a branch or tag"),
            ("h", "jump to the checked-out commit"),
            ("enter", "review the commit in tuicr"),
            (
                "space",
                "check out a branch of the commit (reset to the remote asks first)",
            ),
            (
                "R",
                "reset the current branch to this commit: soft, mixed or hard (asks first)",
            ),
            (
                "r",
                "rebase the current branch onto this commit or its branch (asks first)",
            ),
            ("x", "abort a rebase in progress"),
            (
                "f",
                "fetch now (kraken also fetches every minute and follows outside changes)",
            ),
            ("tab", "focus the detail panel"),
            ("d", "show / hide the detail panel"),
        ],
    ),
    (
        "Find",
        &[
            ("/", "fuzzy find a commit, branch or tag"),
            ("b", "fuzzy find a branch or tag"),
            ("↑ / ↓", "move in the list"),
            ("enter", "jump to it"),
            ("shift-enter", "check out the highlighted branch"),
            ("esc", "close the list"),
        ],
    ),
    (
        "Detail panel",
        &[
            ("j / k", "move through the changed files"),
            ("enter", "review the commit in tuicr"),
            ("tab / esc", "back to the graph"),
        ],
    ),
    (
        "General",
        &[
            ("?", "show / hide this help"),
            ("esc / backspace", "back"),
            ("q", "quit"),
        ],
    ),
];

fn draw_help(frame: &mut Frame, full: Rect, scroll: usize) {
    let rows: u16 = HELP.iter().map(|(_, keys)| keys.len() as u16 + 2).sum();
    let width = 92.min(full.width);
    let height = (rows + 2).min(full.height);
    let area = Rect {
        x: full.x + (full.width - width) / 2,
        y: full.y + (full.height - height) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Keybindings ")
        .border_style(Style::default().fg(Color::Rgb(100, 140, 230)));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let mut lines = Vec::new();
    for (section, keys) in HELP {
        lines.push(Line::from(Span::styled(
            format!(" {section}"),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )));
        for (key, description) in *keys {
            lines.push(Line::from(vec![
                Span::styled(format!("   {key:<17}"), Style::default().fg(MATCH)),
                Span::styled(description.to_string(), Style::default().fg(PANEL_TEXT)),
            ]));
        }
        lines.push(Line::default());
    }
    let overflow = lines.len().saturating_sub(inner.height as usize);
    let skipped = scroll.min(overflow);
    frame.render_widget(
        Paragraph::new(lines.into_iter().skip(skipped).collect::<Vec<_>>()),
        inner,
    );
}

fn draw_status(frame: &mut Frame, app: &App, context: &ViewContext, area: Rect) {
    let head = match &app.refs.head {
        Head::Branch(name) => name.clone(),
        Head::Detached(hash) => format!("detached {}", hash.chars().take(7).collect::<String>()),
    };
    let loading = if app.loading_done {
        ""
    } else {
        " (loading…)"
    };
    let sync = if app.sync_note.is_empty() {
        String::new()
    } else {
        format!(" · {}", app.sync_note)
    };
    let text = format!(
        " {} · {} · {} commits{}{}  {}",
        context.repo_name,
        head,
        app.commits.len(),
        loading,
        sync,
        app.status
    );
    let mut spans = Vec::new();
    if app.rebasing {
        spans.push(Span::styled(
            " REBASING · resolve the conflicts, then git rebase --continue · x abort ",
            Style::default()
                .fg(Color::White)
                .bg(Color::Rgb(170, 50, 50))
                .add_modifier(Modifier::BOLD),
        ));
    }
    spans.push(Span::styled(
        text,
        Style::default().fg(HEADER).bg(Color::Rgb(28, 30, 40)),
    ));
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_picker(frame: &mut Frame, app: &App, full: Rect, remote_icon: &str) {
    let Some(picker) = &app.picker else {
        return;
    };
    let width = (full.width / 2).max(40).min(full.width);
    let height = (full.height * 2 / 3).max(6).min(full.height);
    let area = Rect {
        x: full.x + (full.width - width) / 2,
        y: full.y + (full.height - height) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(match picker.purpose {
            PickerPurpose::Jump => " Jump to branch or tag ",
            PickerPurpose::Find => " Find a commit, branch or tag ",
            PickerPurpose::Checkout => " Check out a branch ",
            PickerPurpose::Reset => " Reset the current branch to this commit ",
        })
        .border_style(Style::default().fg(Color::Rgb(100, 140, 230)));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let mut lines = vec![Line::from(Span::styled(
        format!("filter: {}▌", picker.filter),
        Style::default().fg(PANEL_TEXT),
    ))];
    let visible = picker.visible();
    let room = inner.height.saturating_sub(1) as usize;
    let start = picker.cursor.saturating_sub(room.saturating_sub(1));
    for (position, (entry, matches)) in visible.iter().enumerate().skip(start).take(room) {
        let base = if position == picker.cursor {
            Style::default().bg(SELECTED).fg(Color::White)
        } else {
            Style::default().fg(PANEL_TEXT)
        };
        let icon = match entry.kind {
            EntryKind::Commit => COMMIT_ICON,
            EntryKind::Local => BRANCH_ICON,
            EntryKind::Remote => remote_icon,
            EntryKind::Tag => TAG_ICON,
            EntryKind::Action => RESET_ICON,
        };
        let mut spans = vec![Span::styled(format!(" {icon} "), base.fg(HEADER))];
        spans.extend(entry.label.chars().enumerate().map(|(index, character)| {
            let style = if matches.contains(&index) {
                base.fg(MATCH).add_modifier(Modifier::BOLD)
            } else {
                base
            };
            Span::styled(character.to_string(), style)
        }));
        if !entry.detail.is_empty() {
            spans.push(Span::styled(format!("  {}", entry.detail), base.fg(HEADER)));
        }
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::style::Color;

    use super::*;
    use crate::git::{ChangedFile, Commit, Head, Person, Ref, RefKind, Refs};
    use crate::ui::state::{Action, PendingAction};

    fn commit(hash: &str, parents: &[&str], subject: &str) -> Commit {
        let person = Person {
            name: "Ada Lovelace".to_string(),
            email: "ada@example.com".to_string(),
            time: 1_791_633_600,
        };
        Commit {
            hash: hash.to_string(),
            parents: parents.iter().map(|parent| parent.to_string()).collect(),
            author: person.clone(),
            committer: person,
            subject: subject.to_string(),
        }
    }

    fn app() -> App {
        let refs = vec![
            Ref {
                name: "main".to_string(),
                kind: RefKind::Local,
                target: "c0".to_string(),
            },
            Ref {
                name: "origin/main".to_string(),
                kind: RefKind::Remote {
                    remote: "origin".to_string(),
                },
                target: "c0".to_string(),
            },
            Ref {
                name: "feature".to_string(),
                kind: RefKind::Local,
                target: "c1".to_string(),
            },
        ];
        let mut app = App::new(Refs {
            refs,
            head: Head::Branch("main".to_string()),
        });
        app.add_commits(
            vec![
                commit("c0", &["c2"], "Merge-free subject"),
                commit("c1", &["c2"], "Feature work"),
                commit("c2", &[], "Initial commit"),
            ],
            10,
        );
        app
    }

    fn app_with_tag() -> App {
        let mut refs = app().refs;
        refs.refs.push(Ref {
            name: "v1.0".to_string(),
            kind: RefKind::Tag,
            target: "c0".to_string(),
        });
        let mut app = App::new(refs);
        app.add_commits(
            vec![
                commit("c0", &["c2"], "Merge-free subject"),
                commit("c1", &["c2"], "Feature work"),
                commit("c2", &[], "Initial commit"),
            ],
            10,
        );
        app
    }

    fn render(app: &App, width: u16, detail: Option<&Detail>) -> ratatui::buffer::Buffer {
        render_sized(app, width, 20, detail)
    }

    fn render_sized(
        app: &App,
        width: u16,
        height: u16,
        detail: Option<&Detail>,
    ) -> ratatui::buffer::Buffer {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        let context = ViewContext {
            graph: GraphWidget::Text,
            detail,
            remote_icon: "G",
            ref_logos: None,
            repo_name: "kraken",
        };
        terminal
            .draw(|frame| draw(frame, app, &context))
            .expect("draw");
        terminal.backend().buffer().clone()
    }

    fn line(buffer: &ratatui::buffer::Buffer, y: u16) -> String {
        (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol().to_string())
            .collect()
    }

    fn detail() -> Detail {
        Detail {
            message: "Merge-free subject\n\nLonger body".to_string(),
            files: vec![
                ChangedFile {
                    status: 'M',
                    path: "src/main.rs".to_string(),
                },
                ChangedFile {
                    status: 'A',
                    path: "src/new.rs".to_string(),
                },
            ],
        }
    }

    #[test]
    fn header_names_the_columns() {
        let buffer = render(&app(), 160, None);
        let header = line(&buffer, 0);
        for title in ["BRANCH / TAG", "GRAPH", "COMMIT MESSAGE", "DATE"] {
            assert!(header.contains(title), "{title} missing in {header:?}");
        }
    }

    #[test]
    fn rows_show_ref_label_extra_count_message_and_date() {
        let mut app = app_with_tag();
        app.apply(Action::Down, 10);
        let buffer = render(&app, 160, None);
        let first = line(&buffer, 1);
        assert!(first.contains("✓ main"), "{first:?}");
        assert!(first.contains("+1"), "{first:?}");
        assert!(first.contains("Merge-free subject"), "{first:?}");
        assert!(first.contains("10/10"), "{first:?}");
        assert!(line(&buffer, 2).contains("feature"));
    }

    #[test]
    fn selected_row_is_highlighted_across_the_message() {
        let mut app = app();
        app.apply(crate::ui::state::Action::Down, 10);
        let buffer = render(&app, 160, None);
        let message_x = line(&buffer, 2).find("Feature work").expect("message") as u16;
        assert_ne!(buffer[(message_x, 2)].bg, Color::Reset);
        assert_eq!(buffer[(message_x, 3)].bg, Color::Reset);
    }

    #[test]
    fn detail_panel_lists_the_changed_files() {
        let buffer = render(&app(), 160, Some(&detail()));
        let screen: String = (0..buffer.area.height)
            .map(|y| line(&buffer, y))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(screen.contains("2 modified"), "{screen}");
        assert!(screen.contains("M src/main.rs"), "{screen}");
        assert!(screen.contains("Ada Lovelace"), "{screen}");
    }

    #[test]
    fn detail_panel_is_hidden_on_a_narrow_terminal() {
        let buffer = render(&app(), 100, Some(&detail()));
        let screen: String = (0..buffer.area.height)
            .map(|y| line(&buffer, y))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!screen.contains("2 modified"));
    }

    #[test]
    fn tall_rows_give_each_commit_two_lines_with_author_and_hash_below() {
        let mut app = app();
        app.row_lines = 2;
        let buffer = render(&app, 160, None);
        assert!(line(&buffer, 1).contains("Merge-free subject"));
        assert!(
            line(&buffer, 2).contains("Ada Lovelace · c0"),
            "{:?}",
            line(&buffer, 2)
        );
        assert!(line(&buffer, 3).contains("Feature work"));
    }

    #[test]
    fn file_under_the_cursor_is_highlighted_when_the_panel_has_focus() {
        let mut app = app();
        app.file_count = 2;
        app.apply(Action::FocusNext, 10);
        app.apply(Action::Down, 10);
        let buffer = render(&app, 160, Some(&detail()));
        let row = (0..buffer.area.height)
            .find(|&y| line(&buffer, y).contains("A src/new.rs"))
            .expect("file row");
        let x = line(&buffer, row).find("src/new.rs").expect("path") as u16;
        assert_ne!(buffer[(x, row)].bg, Color::Reset);
    }

    #[test]
    fn tall_rows_show_two_refs_without_a_counter() {
        let mut app = app_with_tag();
        app.row_lines = 2;
        app.apply(Action::Down, 10);
        let buffer = render(&app, 160, None);
        assert!(
            line(&buffer, 1).contains("✓ main"),
            "{:?}",
            line(&buffer, 1)
        );
        assert!(line(&buffer, 2).contains("v1.0"), "{:?}", line(&buffer, 2));
        assert!(!line(&buffer, 1).contains("+1"));
    }

    #[test]
    fn local_branch_and_its_remote_share_one_label_ending_with_both_icons() {
        let buffer = render(&app(), 160, None);
        assert!(
            line(&buffer, 1).contains(&format!("✓ main {LOCAL_ICON} G")),
            "{:?}",
            line(&buffer, 1)
        );
    }

    #[test]
    fn local_only_branch_ends_with_the_local_icon_alone() {
        let buffer = render(&app(), 160, None);
        assert!(
            line(&buffer, 2).contains(&format!("feature {LOCAL_ICON} ")),
            "{:?}",
            line(&buffer, 2)
        );
    }

    #[test]
    fn remote_only_branch_drops_the_remote_prefix_and_ends_with_the_remote_icon() {
        let mut refs = app().refs;
        refs.refs.push(Ref {
            name: "origin/review".to_string(),
            kind: RefKind::Remote {
                remote: "origin".to_string(),
            },
            target: "c1".to_string(),
        });
        let mut app = App::new(refs);
        app.add_commits(
            vec![
                commit("c0", &["c2"], "Merge-free subject"),
                commit("c1", &["c2"], "Feature work"),
                commit("c2", &[], "Initial commit"),
            ],
            10,
        );
        app.apply(Action::Down, 10);
        let buffer = render(&app, 160, None);
        assert!(
            line(&buffer, 3).contains("review G"),
            "{:?}",
            line(&buffer, 3)
        );
    }

    #[test]
    fn pixel_logos_get_two_blank_cells_each_right_after_the_name() {
        let app = app();
        let logos = RefLogos {
            local: Vec::new(),
            remote: Vec::new(),
        };
        let context = ViewContext {
            graph: GraphWidget::Text,
            detail: None,
            remote_icon: "G",
            ref_logos: Some(&logos),
            repo_name: "kraken",
        };
        let badge = &ref_badges(&app.refs_at(0))[0];
        let label = ref_label(&app, badge, "", 40, 0, &context);
        assert_eq!(label.marks, vec![(7, Mark::Local), (10, Mark::Remote)]);
    }

    #[test]
    fn selected_row_expands_to_list_every_ref_and_pushes_the_next_rows_down() {
        let app = app_with_tag();
        let buffer = render(&app, 160, None);
        assert!(
            line(&buffer, 1).contains("✓ main"),
            "{:?}",
            line(&buffer, 1)
        );
        assert!(line(&buffer, 2).contains("v1.0"), "{:?}", line(&buffer, 2));
        assert!(
            line(&buffer, 3).contains("Feature work"),
            "{:?}",
            line(&buffer, 3)
        );
    }

    fn screen_text(buffer: &ratatui::buffer::Buffer) -> String {
        (0..buffer.area.height)
            .map(|y| line(buffer, y))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn confirmation_is_shown_with_a_default_no() {
        let mut app = app();
        app.confirm = Some(crate::ui::state::Confirmation {
            message: "Reset staging to origin/staging?".to_string(),
            action: PendingAction::Checkout("origin/staging".to_string()),
        });
        let text = screen_text(&render(&app, 160, None));
        assert!(text.contains("Reset staging to origin/staging?"), "{text}");
        assert!(text.contains("[y/N]"), "{text}");
    }

    #[test]
    fn checkout_list_has_its_own_title() {
        let mut app = app();
        app.apply(Action::Checkout, 10);
        let text = screen_text(&render(&app, 160, None));
        assert!(text.contains("Check out a branch"), "{text}");
    }

    #[test]
    fn checked_out_commit_has_its_own_background_when_not_selected() {
        let mut app = app();
        app.apply(Action::Down, 10);
        let buffer = render(&app, 160, None);
        let x = line(&buffer, 1)
            .find("Merge-free subject")
            .expect("message") as u16;
        assert_eq!(buffer[(x, 1)].bg, HEAD_ROW);
        let selected_x = line(&buffer, 2).find("Feature work").expect("message") as u16;
        assert_eq!(buffer[(selected_x, 2)].bg, SELECTED);
    }

    #[test]
    fn finder_shows_commit_details_and_highlights_matched_letters() {
        let mut app = app();
        app.apply(Action::OpenFinder, 10);
        for character in "fwork".chars() {
            app.apply(Action::PickerChar(character), 10);
        }
        let buffer = render(&app, 160, None);
        let row = (0..buffer.area.height)
            .find(|&y| line(&buffer, y).contains("Feature work  c1"))
            .expect("finder row");
        assert!(
            line(&buffer, row).contains("c1"),
            "{:?}",
            line(&buffer, row)
        );
        let text = line(&buffer, row);
        let x = text[..text.find("Feature work").expect("label")]
            .chars()
            .count() as u16;
        assert_eq!(buffer[(x, row)].fg, MATCH);
        assert_ne!(buffer[(x + 1, row)].fg, MATCH);
    }

    fn last_line(buffer: &ratatui::buffer::Buffer) -> String {
        line(buffer, buffer.area.height - 1)
    }

    #[test]
    fn hint_bar_follows_the_context() {
        let mut app = app();
        assert!(last_line(&render(&app, 160, None)).contains("? help"));
        app.apply(Action::OpenFinder, 10);
        assert!(last_line(&render(&app, 160, None)).contains("shift-enter check out"));
        app.apply(Action::Back, 10);
        app.apply(Action::FocusNext, 10);
        assert!(last_line(&render(&app, 160, None)).contains("j/k files"));
        app.confirm = Some(crate::ui::state::Confirmation {
            message: "Reset?".to_string(),
            action: PendingAction::Checkout("x".to_string()),
        });
        assert!(last_line(&render(&app, 160, None)).contains("y confirm"));
    }

    #[test]
    fn help_screen_lists_the_keybindings() {
        let mut app = app();
        app.apply(Action::ToggleHelp, 10);
        let text = screen_text(&render_sized(&app, 160, 50, None));
        for expected in [
            "Keybindings",
            "space",
            "shift-enter",
            "check out",
            "fuzzy find",
        ] {
            assert!(text.contains(expected), "{expected} missing:\n{text}");
        }
    }

    #[test]
    fn rebase_in_progress_shows_a_banner_with_the_abort_key() {
        let mut app = app();
        app.rebasing = true;
        let buffer = render(&app, 160, None);
        let status = line(&buffer, buffer.area.height - 2);
        assert!(status.contains("REBASING"), "{status:?}");
        assert!(status.contains("x abort"), "{status:?}");
    }

    #[test]
    fn help_and_hints_mention_reset_and_rebase() {
        let mut app = app();
        assert!(last_line(&render(&app, 200, None)).contains("R reset"));
        app.apply(Action::ToggleHelp, 10);
        let text = screen_text(&render_sized(&app, 160, 50, None));
        assert!(text.contains("rebase the current branch"), "{text}");
        assert!(text.contains("reset the current branch"), "{text}");
    }

    #[test]
    fn help_scrolls_when_it_does_not_fit() {
        let mut app = app();
        app.apply(Action::ToggleHelp, 10);
        assert!(!screen_text(&render(&app, 160, None)).contains("General"));
        for _ in 0..30 {
            app.apply(Action::Down, 10);
        }
        assert!(screen_text(&render(&app, 160, None)).contains("General"));
    }

    #[test]
    fn status_bar_shows_the_sync_note() {
        let mut app = app();
        app.sync_note = "fetched 14:03".to_string();
        let buffer = render(&app, 160, None);
        assert!(line(&buffer, buffer.area.height - 2).contains("fetched 14:03"));
    }

    #[test]
    fn graph_column_keeps_a_minimum_share_of_the_width() {
        let app = app();
        assert_eq!(graph_columns(&app, 10, 100), 18);
        assert_eq!(graph_columns(&app, 10, 5), 2);
    }
}
