//! A made-up company for the documentation screenshots: people, repositories and history.
//! Nothing here is real; every name and address is invented, and everything lives in a TempDir.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A made-up author. `zone` is the UTC offset git writes after the time.
pub struct Person {
    pub name: &'static str,
    pub email: &'static str,
    pub zone: &'static str,
}

const fn person(name: &'static str, email: &'static str, zone: &'static str) -> Person {
    Person { name, email, zone }
}

pub const PEOPLE: [Person; 12] = [
    person("Mira Okafor", "mira.okafor@northwind.dev", "+0100"),
    person("Jonas Lindqvist", "jonas.lindqvist@northwind.dev", "+0200"),
    person("Priya Raman", "priya.raman@northwind.dev", "+0530"),
    person("Tomás Herrera", "tomas.herrera@northwind.dev", "-0300"),
    person("Anya Volkova", "anya.volkova@northwind.dev", "+0300"),
    person("Liam O'Connell", "liam.oconnell@northwind.dev", "+0000"),
    person("Sofia Marchetti", "sofia.marchetti@northwind.dev", "+0200"),
    person("Kenji Watanabe", "kenji.watanabe@northwind.dev", "+0900"),
    person("Aisha Rahman", "aisha.rahman@northwind.dev", "+0400"),
    person("Noah Fischer", "noah.fischer@northwind.dev", "+0100"),
    person("Chloe Dubois", "chloe.dubois@northwind.dev", "-0500"),
    person("Mateo Alvarez", "mateo.alvarez@northwind.dev", "-0800"),
];

/// The user of the screenshots: the one whose name the Summary pane shows.
pub const ME: Person = person("Elena Brandt", "elena.brandt@northwind.dev", "+0100");

/// Seconds since 1970 for 2026-10-17 18:00 UTC, the "now" of the made-up history.
const NOW: i64 = 1_792_260_000;

/// The same numbers on every run, so every run draws the same people.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

}

/// git with a fixed, private environment: no global config, no ssh, a fake home.
pub struct Git {
    pub home: PathBuf,
}

impl Git {
    pub fn new(home: &Path) -> Git {
        fs::create_dir_all(home).expect("home");
        Git {
            home: home.to_path_buf(),
        }
    }

    fn command(&self, dir: &Path) -> Command {
        let mut cmd = Command::new("git");
        cmd.current_dir(dir)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("HOME", &self.home)
            .env("GIT_TERMINAL_PROMPT", "0");
        cmd
    }

    pub fn run(&self, dir: &Path, args: &[&str]) -> String {
        let out = self.command(dir).args(args).output().expect("run git");
        assert!(
            out.status.success(),
            "git {args:?} in {} failed: {}",
            dir.display(),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// `git <args>` as `who` at `days_ago` days (and `hour` o'clock UTC) before the made-up now.
    fn run_as(&self, dir: &Path, who: &Person, days_ago: i64, hour: i64, args: &[&str]) {
        let secs = NOW - days_ago * 86_400 - (18 - hour) * 3_600;
        let when = format!("{secs} {}", who.zone);
        let out = self
            .command(dir)
            .args(args)
            .env("GIT_AUTHOR_NAME", who.name)
            .env("GIT_AUTHOR_EMAIL", who.email)
            .env("GIT_AUTHOR_DATE", &when)
            .env("GIT_COMMITTER_NAME", who.name)
            .env("GIT_COMMITTER_EMAIL", who.email)
            .env("GIT_COMMITTER_DATE", &when)
            .output()
            .expect("run git");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// Stages everything and commits it.
    pub fn commit(&self, dir: &Path, who: &Person, days_ago: i64, hour: i64, message: &str) {
        self.run(dir, &["add", "--all"]);
        self.run_as(
            dir,
            who,
            days_ago,
            hour,
            &["commit", "--allow-empty", "-q", "-m", message],
        );
    }

    /// A merge commit that keeps both lanes in the graph.
    pub fn merge(&self, dir: &Path, who: &Person, days_ago: i64, branch: &str, message: &str) {
        self.run_as(
            dir,
            who,
            days_ago,
            11,
            &["merge", "--no-ff", "-q", "-m", message, branch],
        );
    }

    pub fn tag(&self, dir: &Path, who: &Person, days_ago: i64, name: &str, message: &str) {
        self.run_as(dir, who, days_ago, 12, &["tag", "-a", name, "-m", message]);
    }
}

pub fn write(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    fs::write(path, text).expect("write file");
}

/// One commit of a small repository: who, how long ago, the message and the files it writes.
pub struct Step {
    pub who: usize,
    pub days_ago: i64,
    pub message: &'static str,
    pub files: &'static [(&'static str, &'static str)],
}

/// Applies `steps` in order to the checkout at `dir`.
pub fn apply(git: &Git, dir: &Path, steps: &[Step]) {
    for step in steps {
        for (rel, text) in step.files {
            write(dir, rel, text);
        }
        let hour = 9 + (step.days_ago * 7 % 9);
        git.commit(dir, &PEOPLE[step.who], step.days_ago, hour, step.message);
    }
}

/// Turns the checkout `seed` into `<root>/.bare` plus one folder per branch in `worktrees`, the
/// layout HydraMerge reads, and gives each branch in `remote_branches` a remote-tracking ref.
pub fn layout(git: &Git, seed: &Path, root: &Path, worktrees: &[&str], remote_branches: &[&str]) {
    fs::create_dir_all(root).expect("repo root");
    let bare = root.join(".bare");
    let bare_arg = format!("--git-dir={}", bare.display());
    let seed_str = seed.to_str().expect("utf8");
    git.run(
        root,
        &["clone", "--bare", "-q", seed_str, bare.to_str().expect("utf8")],
    );
    // Whoever works in the made-up repositories is Elena: she is the author the Summary shows.
    for (key, value) in [("user.name", ME.name), ("user.email", ME.email)] {
        git.run(root, &[&bare_arg, "config", key, value]);
    }
    for branch in worktrees {
        let dest = root.join(branch);
        git.run(
            root,
            &[&bare_arg, "worktree", "add", "-q", dest.to_str().expect("utf8"), branch],
        );
    }
    for branch in remote_branches {
        let target = format!("refs/remotes/origin/{branch}");
        git.run(root, &[&bare_arg, "update-ref", &target, branch]);
    }
}
