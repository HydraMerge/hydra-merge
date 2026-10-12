//! Every process launch lives here: git and the Sublime Text CLI, argv only.

mod changes;
mod compare;
mod editor;
mod network;
mod process;
mod refs;
mod worktrees;

pub use changes::*;
pub use compare::*;
pub use editor::*;
pub use network::*;
pub use process::*;
pub use refs::*;
pub use worktrees::*;

use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use crate::diff::TRUNCATED_MARKER;
use crate::discover::{
    branch_syntax_error, ensure_under_roots, layout_path_for, resolve_repo,
    resolve_worktree_for_launch, resolve_worktree_with,
};
use crate::model::{
    AppError, AppResult, Config, ErrorCode, Layout, Repo, Worktree, BARE_DIRNAME, GIT_TIMEOUT,
    NETWORK_TIMEOUT, SMERGE_TIMEOUT,
};

pub(crate) const REFS_HEADS: &str = "refs/heads/";
