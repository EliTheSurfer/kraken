use kraken::git::{CommitStream, Repo};
use kraken::graph::Layout;
use kraken::render::text::render_row;

fn main() -> anyhow::Result<()> {
    let path = std::env::args().nth(1).unwrap_or_else(|| ".".to_string());
    let count: usize = std::env::args()
        .nth(2)
        .and_then(|value| value.parse().ok())
        .unwrap_or(40);
    let repo = Repo::discover(std::path::Path::new(&path))?;
    let mut stream = CommitStream::spawn(&repo)?;
    let mut layout = Layout::new();
    for commit in stream.next_batch(count)? {
        let row = layout.push(&commit.hash, &commit.parents);
        println!(
            "{:<24} {}",
            render_row(&row),
            commit.subject.chars().take(70).collect::<String>()
        );
    }
    Ok(())
}
