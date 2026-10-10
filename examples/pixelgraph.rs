use kraken::git::{CommitStream, Repo};
use kraken::graph::Layout;
use kraken::render::pixels::{Geometry, NodeLook, draw_window, visible_lanes};

fn main() -> anyhow::Result<()> {
    let mut arguments = std::env::args().skip(1);
    let path = arguments.next().unwrap_or_else(|| ".".to_string());
    let output = arguments.next().unwrap_or_else(|| "graph.png".to_string());
    let repo = Repo::discover(std::path::Path::new(&path))?;
    let mut layout = Layout::new();
    let commits = CommitStream::spawn(&repo)?.next_batch(24)?;
    let rows: Vec<_> = commits
        .iter()
        .map(|commit| layout.push(&commit.hash, &commit.parents))
        .collect();
    let nodes: Vec<NodeLook> = commits
        .iter()
        .map(|commit| NodeLook {
            avatar: None,
            author_color: kraken::avatars::author_color(&commit.author.email),
        })
        .collect();
    let geometry = Geometry::for_cell(18.0, 40.0);
    draw_window(&rows, &nodes, visible_lanes(&rows, 12), geometry, &[]).save(&output)?;
    Ok(())
}
