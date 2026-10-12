# HydraMerge

![HydraMerge](assets/logo.png)

Desktop app (Rust, `eframe`/`egui`) for repositories laid out as "bare + one folder per branch",
on the current macOS, Windows and Linux, each as x86_64 and arm64.
It lists every such repository under the discovery roots, shows history,
changes and diffs of each worktree, creates and removes worktrees, and opens them in Sublime Text, VS Code or
another editor, or in Terminal.
Contract: [docs/spec.md](docs/spec.md).

```
<repo_root>/.bare/        bare git dir
<repo_root>/<branch>/     worktree for that branch (feature/foo -> <repo_root>/feature/foo)
```

## Build and run

```
cargo run --release
# or
cargo build --release && target/release/hydra-merge
```

Needs Rust stable (edition 2021, built with 1.99) and git 2.20+. The app is a native window: it runs no server
and has no command-line flags. A release build for this machine is `cargo build --release`. Every push to `main`
builds the six release binaries and uploads them as GitHub Actions artifacts. A tag `v*` publishes them on a GitHub Release. The macOS archive is a zip of `HydraMerge.app`; Linux and Windows archives contain the `hydra-merge` binary:

| OS | x86_64 | arm64 |
|---|---|---|
| macOS | `x86_64-apple-darwin` | `aarch64-apple-darwin` |
| Linux | `x86_64-unknown-linux-gnu` | `aarch64-unknown-linux-gnu` |
| Windows | `x86_64-pc-windows-msvc` | `aarch64-pc-windows-msvc` |

On Linux the window needs X11 or Wayland libraries (`libxkbcommon`, X11, Wayland). On Windows and Linux,
`Open in Terminal` uses Windows Terminal or `x-terminal-emulator` unless `HYDRA_MERGE_TERMINAL` names another
program. The folder picker uses the system dialog (`osascript`, `zenity`/`kdialog`, or PowerShell).

## Environment

| Variable | Meaning | Default |
|---|---|---|
| `HYDRA_MERGE_ROOTS` | discovery roots, separated by `:` on Unix and `;` on Windows (`~` is expanded, missing paths are skipped) | `~/repos` |
| `HYDRA_MERGE_SMERGE` | path to the `subl` executable, or the name `subl` on `PATH` | macOS: `/Applications/Sublime Text.app/Contents/SharedSupport/bin/subl`; Linux: `subl`; Windows: `%ProgramFiles%\Sublime Text\subl.exe` |
| `HYDRA_MERGE_TERMINAL` | program used by "Open in Terminal" (for example `iTerm`, `kitty`, `wt.exe`) | macOS `Terminal`, Linux `x-terminal-emulator`, Windows `wt.exe` |

## Features

### Repositories and worktrees
- **Discovery** of every `<dir>/.bare` bare repository under the roots (depth 4), on a few threads. The list
  appears at once; the clean/modified state of every worktree is read in the background.
- **Open Repository** (+ next to the tabs, ⚙ or the palette) opens a window: type a path or press **Choose…** for
  the macOS folder picker. A `.bare` layout or a plain clone under the discovery roots opens as a tab; a reason
  why a folder cannot be opened is shown in the window.
- **Create a worktree** at `<repo_root>/<branch>` for a free local branch, or from a remote branch
  (`worktree add --track -b`). A branch already checked out elsewhere leads to its worktree instead.
- **Create a local branch** from a remote branch (`git branch --track`) without a worktree.
- **Remove** a clean, unlocked worktree after confirmation. Dirty or locked worktrees are refused.
- **Delete a remote branch** (`git push --delete <remote> refs/heads/<branch>`) after confirmation.
- **Open** a worktree in your editor, Terminal or Finder. **⚙ → Open With** (or "Open With:" in the palette) picks
  the editor for Open and every "Open in …": Sublime Text (the `subl` CLI), Visual Studio Code, Cursor, Zed and
  JetBrains IDEs when installed, or **Other App…** — any application, chosen in the macOS picker. Apps are started
  with `open -a <app> <folder>`, so no command-line tool is needed. The choice is kept in the session.

### Window
- **Repository tabs** with a folder icon and ×, square and side by side, no scroll bar: the mouse wheel and the
  trackpad scroll them. ▾ lists all repositories and reopens closed tabs; + opens the Open Repository window.
- **Toolbar**: Locations toggle, ◀ ▶ back / forward, the command palette field with the current branch and a
  clean/modified dot, ⋯ for the shown worktree (Copy Worktree Path, Reveal in Finder, Open in <editor>,
  Open in Terminal, Remove Worktree…), Fetch ↻ / Pull ↓ / Push ↑ / Open, and ⚙ settings (Rescan, Open Repository
  by Path…, Scan at Startup, Fetch Automatically, theme, Open With, Skin).
- **Locations**: Worktrees, Branches, Remotes, Tags, Stashes, Submodules. Sections collapse, branch names with
  `/` are grouped into folders, and a filter narrows the list. Right click offers what fits the item: create a
  worktree or local branch, compare, delete a remote branch, open, copy the name or path.
- **Commit list**: commits of all branches with the branch graph (merges, lanes), ref badges (checked-out branch,
  local, remote, tag), author, date and file count. The top row is the working copy ("2 unstaged files").
  The history column is resized by dragging its divider. Right click a commit to copy its hash or subject.
- **Summary** of the working copy: commit message, author from `git config`, Commit button, and collapsible
  Staged / Unstaged / Untracked sections, each with its diff.
- **Commit view**: subject, author, hash, files and the patch.
- **Diff**: line numbers, added and removed lines, syntax colours for common languages and config formats
  (Rust, Go, Python, JS/TS, YAML, HCL/Terraform, JSON, SQL, shell, …). Only visible lines are drawn.
- **Skins** (⚙ → Skin, or "Skin:" in the palette): Auto (Merge Light / Merge Dark, following the system and the
  light/dark switch), Merge Light, Merge Dark, Mariana, One Dark, Monokai, Nord, Solarized Dark, Solarized Light,
  GitHub Light, and your own skin files — see [Skins](#skins).

### Changes and sync
- **Stage / Unstage / Stage All / Discard** (tracked files, after confirmation) and **Commit**.
- **Several files at once**: click, ⌘-click and Shift-click in Summary, then Stage / Unstage / Discard Selected.
  Right click a file for "Stage Folder".
- **Fetch, Pull** (`--no-edit`) and **Push** of the selected worktree. No force-push.
- **Merge request link**: when the server prints one after a push (GitLab, GitHub, Bitbucket), the status line
  offers "Open Merge Request on <host> ↗".
- **Sign-in errors** (no credential helper, SSH key missing) are named as such and offer "Open in Terminal" to
  sign in once; the app never shows a credential prompt.
- **Stashes** can be applied or dropped (drop after confirmation).

### Finding things
- **Command palette** (click the field or ⌘⇧P): fuzzy search over every action — sync, open, stage, commit,
  switch repository or worktree, create a worktree, compare, delete a remote branch, settings. An empty query
  shows the last commands run.
- **Back / Forward** (◀ ▶, ⌘[ / ⌘]) through the repositories, worktrees, commits and Locations items shown before.
- **Filter commits** (🔍 in the Commits header or ⌘F) by message, author, hash or branch; "Search all history"
  runs `git log --grep` over every branch.
- **Compare** the shown worktree's branch with another branch: `git diff <current>...<other>`, as a merge request
  shows it.

### Background
- **Outside changes** (a worktree added, moved or removed in a terminal, a branch moved, something staged) are
  noticed within seconds and only that repository is reloaded. The open worktree is read again when the window
  gets focus.
- **Auto-fetch** (⚙ → Fetch Automatically): `git fetch --all` of the open repository every 5 minutes; a failure
  only shows in the status line. A repository that is not trusted (below) is skipped.
- **Trust**: a repository that arrives as a whole folder (an archive, a copy) brings its own git config and hooks,
  and git runs what they name. When the repository's own config or hooks can start a program (`filter.*.clean`,
  `filter.*.smudge`, `filter.*.process`, `merge.*.driver`, `core.sshCommand`, `core.gitProxy`, `core.hooksPath`,
  `core.askPass`, `credential.*helper`, `remote.*.uploadpack`, `protocol.*.allow`, per-worktree `config.worktree`, or an
  executable hook in `hooks/`), the tab shows **This repository is not trusted** with the list of what can run, and
  nothing of it is read or changed: no status, history, diff, fetch, stage or commit, and no auto-fetch. Opening it
  in an editor or Terminal still works. **Trust this repository** remembers the decision in `session.txt` for exactly
  that config: change a value, add a hook, and you are asked again. ⌘⇧P → *Stop Trusting This Repository* takes it
  back. Repositories without such settings need no decision.
- **Repositories are data**: every git call also switches off `core.fsmonitor` and `log.showSignature` (a plain read
  would otherwise run a program from the repository's config), diffs run without `textconv`, and file names from
  Stage, Unstage and Discard are matched literally (`--literal-pathspecs`).
- **Responsive**: git runs on background threads. Loading a tab never blocks actions, a click during a running
  task waits in a queue, and revisited tabs show cached data at once while it is read again.
- **Session**: window size and place, tabs (also closed ones), the open repository and worktree, the history
  column width, Locations shown or hidden, auto-fetch and recent palette commands are restored at the next start.
- **Tools from Finder**: git runs with `/opt/homebrew/bin`, `/usr/local/bin` and `/etc/paths.d` added to `PATH`,
  so hooks (husky), git-lfs and gpg work when the app is started from Finder or the Dock.

## Skins

A skin is a palette: every colour of the window, the diff and the syntax. Pick one in ⚙ → Skin or with
"Skin: …" in the command palette; the choice is kept in `skin.txt`.

**Your own skins** are files in the settings folder `skins/<name>.skin`
(macOS `~/Library/Application Support/hydra-merge/`, Linux `~/.config/hydra-merge/`,
Windows `%APPDATA%\hydra-merge\`).
⚙ → Skin → **Create Skin from Current…** writes the colours in use to a new file and opens it in Sublime Text;
every save shows in the window within a second or two. **Open Skins Folder** shows the folder in Finder.

```
# comments start with '#'
base = Nord                # a skin name, or "dark" / "light"; tokens not set below come from it
dark = true                # optional: dark or light base for egui widgets
accent = #ff8800           # any token as #rrggbb
badge_head_bg = #ff8800
graph = #c678dd, #61afef, #98c379, #d19a66, #e06c75, #e5c07b   # six branch-lane colours
button_disabled_opacity = 0.4
```

Tokens: `bg` (commit list), `detail_bg` (Summary / commit), `sidebar_bg`, `header_bg` (tab strip), `toolbar_bg`,
`pane_header_bg`, `section_bg`, `text`, `text_strong`, `text_muted`, `accent`, `row_selected_bg`, `diff_add_bg`,
`diff_remove_bg`, `syntax_keyword`, `syntax_string`, … — "Create Skin from Current…" lists all of them. A line that
cannot be used (unknown token, bad colour) is reported in the error bar with its line number; the rest still
applies. The light/dark switch returns to Auto.

## Files it writes

All in the settings folder (`~/Library/Application Support/hydra-merge/` on macOS,
`~/.config/hydra-merge/` on Linux, `%APPDATA%\hydra-merge\` on Windows):

| File | Content |
|---|---|
| `theme.txt` | `dark` or `light`; missing means "follow the system" |
| `autoscan.txt` | `on` or `off` (Scan at Startup); missing means off — the first window does not scan until Rescan |
| `session.txt` | the session described above |
| `skin.txt` | the chosen skin's name; missing means Auto |
| `skins/*.skin` | your skins (written only by "Create Skin from Current…") |

## What it never does

- `checkout`, `switch`, `reset`, `clean`, `merge`, `rebase`, `prune`
- `--force` for any git command, including push
- deleting untracked files
- deleting the `.bare` directory or any file on its own
- converting plain clones to the `.bare` layout
- opening `.bare` or the repository root in Sublime Text

Git is started only with `std::process::Command` and an argv. Local git calls time out after 15 s. Fetch, pull,
and push time out after 120 s.

## Keyboard

| Keys | Action |
|---|---|
| `⌘⇧P` / `Ctrl+Shift+P` | command palette |
| `⌘[` / `⌘]` or `Ctrl+[` / `Ctrl+]` | back / forward |
| `⌘F` / `Ctrl+F` | filter commits |
| `⌘R` / `Ctrl+R` | rescan |
| `↑` / `↓`, `Enter` | move through the worktrees in Locations, open the selected one |
| `↑` / `↓`, `Enter`, `Esc` | in the palette: choose, run, close |

While a text field has focus only the palette, back / forward and find shortcuts work; the other keys go to the field. On Windows and Linux those shortcuts use Ctrl instead of ⌘.

## Tests

```
cargo test
cargo clippy --all-targets -- -D warnings
cargo test --test ui_shots -- --ignored   # renders the window offscreen into target/ui-shots/ (GPU required)
```

The tests use temporary repositories only and a fake editor CLI; they do not read `~/repos`, `/Applications`,
or change the process environment.
