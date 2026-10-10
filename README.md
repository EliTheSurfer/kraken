# kraken

A terminal view of a git repository's history, drawn like GitKraken: branch and
tag labels, a colored lane graph with curved merge lines and author avatars, commit
messages, dates, and a detail panel with the files each commit changed.

The graph is drawn in pixels through the kitty graphics protocol (Ghostty, kitty, WezTerm,
and herdr inside a compatible terminal). Other terminals get the same graph drawn with
Unicode lane characters.

## Install

One line, macOS or Linux (Apple Silicon, ARM or Intel), no Rust needed:

```sh
curl -fsSL https://raw.githubusercontent.com/EliTheSurfer/kraken/main/install.sh | sh
```

It downloads the latest release binary into `~/.local/bin` (set `KRAKEN_INSTALL_DIR` to
choose another folder). With Rust installed, you can also build it yourself:

```sh
cargo install --git https://github.com/EliTheSurfer/kraken
```

kraken needs `git`. Optional: [tuicr](https://github.com/agavra/tuicr) to review commits,
`gh` or `glab` for GitHub or GitLab avatars.

## Usage

```sh
kraken            # repository of the current directory
kraken <path>     # another repository
kraken --text     # force the Unicode graph, no images
```

## Keys

| Key | Action |
|---|---|
| `j` / `k`, `↓` / `↑` | next / previous commit |
| `g` / `G` | first / last commit |
| `ctrl-d` / `ctrl-u` | half page down / up |
| `]` / `[` | next / previous commit carrying a branch or tag |
| `/` | fuzzy find a commit (message, hash, author), branch or tag; arrows to move, `enter` to jump |
| `b` | fuzzy find a branch or tag only |
| `shift-enter` | in `/` or `b`, check out the highlighted branch (same rules as `space`) |
| `h` | jump to the checked-out commit |
| `enter` | review the selected commit in [tuicr](https://github.com/agavra/tuicr); kraken comes back when tuicr closes |
| `tab` | focus the detail panel: `j` / `k` move through the changed files, `enter` reviews the commit in tuicr, `tab` again to return |
| `space` | check out a branch of the selected commit (see below) |
| `R` | reset the current branch to the selected commit: soft, mixed or hard |
| `r` | rebase the current branch onto the selected commit (or its branch) |
| `x` | abort a rebase in progress |
| `f` | fetch now |
| `d` | show or hide the detail panel |
| `esc` / `backspace` | back: close the list, leave the panel |
| `?` | show / hide every keybinding |
| `q` | quit |

The bottom line always lists the keys available in the current context.

The detail panel is shown when the terminal is at least 120 columns wide.

Each commit row shows up to two branches or tags, one per line, and `+N` when it has more.
The selected commit's row grows to list all of them.

## Avatars

For each author, in order:

1. GitHub noreply addresses map straight to the GitHub avatar.
2. Repositories hosted on GitHub ask the GitHub API through `gh`, when it is logged in.
   Repositories hosted on GitLab search the author on GitLab through `glab` (matching the
   name's words in any order, ignoring placeholder accounts) and use their uploaded photo.
3. Gravatar, from the commit email.
4. Otherwise a colored circle, the color stable per author.

Avatars, including "not found", are cached in `~/.cache/kraken/avatars/`. Delete that
directory to fetch them again.

## Staying up to date

- kraken runs `git fetch --all --prune` in the background when it starts and then every
  minute, so branches pushed by others show up on their own. `f` fetches immediately. The
  status bar shows `fetching…`, `fetched HH:MM` or the fetch error.
- The fetch never prompts: it runs with `GIT_TERMINAL_PROMPT=0` and SSH in batch mode
  (your `core.sshCommand` is kept). Remotes that need a password simply report an error.
- Every two seconds kraken also compares the repository's refs and HEAD, so a commit,
  checkout or fetch made in another terminal appears within a couple of seconds.
- On any change the history and refs reload and the selection stays on the same commit
  (or moves to HEAD when that commit no longer exists).

## Checking out branches

`space` checks out a branch of the selected commit. When the commit carries several
branches, a list lets you pick one.

| Branch picked | What kraken runs |
|---|---|
| local branch | `git switch <branch>` |
| remote branch with no local branch | `git switch --create <branch> --track <remote>/<branch>` |
| remote branch with a local branch | switch to the local branch if needed, then `git reset --keep <remote>/<branch>` |

The reset always asks for confirmation (`y`, anything else cancels) and says how many
local commits would be dropped. `--keep` preserves uncommitted changes and refuses rather
than overwrite them; dropped commits stay reachable through `git reflog`. When git refuses
an action, its message appears in the status bar.

## Reset and rebase

- `R` opens a menu with the three reset modes. Every mode asks for confirmation and says
  how many commits leave the branch; a hard reset also says how many changed files are
  discarded.
- `r` rebases the current branch onto the selected commit's branch (local first), or onto
  the commit itself when it has none, after confirmation (`git rebase <target>`).
- When a rebase stops on conflicts, the status bar shows a red REBASING banner until the
  rebase ends. Resolve the conflicts and run `git rebase --continue` in your terminal, or
  press `x` in kraken to run `git rebase --abort`.
- After a reset or rebase kraken reloads the history and selects the new HEAD. A long
  rebase keeps kraken busy until git finishes.

Everything else kraken does only reads the repository (`git log`, `git for-each-ref`,
`git show`, `git rev-parse`, `git symbolic-ref`, `git remote get-url`, `git status`), apart
from `git fetch`, which only updates remote-tracking branches.

## Development

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo run --example textgraph -- <repo> 40        # print the lane layout as text
cargo run --example pixelgraph -- <repo> out.png  # render 24 rows of the graph to a PNG
```

## License

MIT, see [LICENSE](LICENSE).
