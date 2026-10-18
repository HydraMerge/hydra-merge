//! The other repositories of the made-up company: short histories by people the random numbers
//! pick, a few of them with an unfinished change so the list shows orange dots.

use std::fs;
use std::path::Path;

use super::demo::{layout, write, Git, Rng, PEOPLE};

struct Theme {
    path: &'static str,
    ext: &'static str,
    /// Commit messages, oldest first.
    messages: &'static [&'static str],
    /// Branches besides `main`; each gets a worktree except the last one.
    branches: &'static [&'static str],
    /// Leave an uncommitted change in the main worktree.
    dirty: bool,
}

const THEMES: [Theme; 12] = [
    Theme { path: "platform/auth-service", ext: "go", messages: &["feat: password login", "feat: refresh tokens", "fix: clock skew in jwt validation", "feat: rate limit failed logins", "chore: rotate signing keys"], branches: &["feature/passkeys", "fix/session-leak"], dirty: true },
    Theme { path: "platform/notification-hub", ext: "ts", messages: &["feat: email channel", "feat: push channel", "fix: dedupe retries", "perf: batch sends per tenant"], branches: &["feature/sms"], dirty: false },
    Theme { path: "apps/web-portal", ext: "ts", messages: &["feat: sign in page", "feat: invoices list", "fix: wrong currency on mobile", "feat: dark mode", "feat: download invoices as pdf", "chore: update dependencies"], branches: &["feature/team-invites", "fix/invoice-total"], dirty: true },
    Theme { path: "apps/mobile-api", ext: "py", messages: &["feat: device registration", "feat: sync endpoint", "fix: pagination off by one", "docs: openapi examples"], branches: &["feature/offline-sync"], dirty: false },
    Theme { path: "apps/admin-console", ext: "ts", messages: &["feat: tenant search", "feat: audit log view", "fix: export button disabled"], branches: &[], dirty: false },
    Theme { path: "infra/terraform-modules", ext: "hcl", messages: &["feat: vpc module", "feat: eks module", "fix: nat gateway per zone", "feat: rds module", "chore: pin provider versions"], branches: &["feature/redis-module"], dirty: true },
    Theme { path: "infra/k8s-manifests", ext: "yaml", messages: &["feat: namespaces", "feat: payments-api deployment", "fix: readiness probe path", "chore: bump resource limits"], branches: &["fix/hpa-targets"], dirty: false },
    Theme { path: "infra/ansible-roles", ext: "yaml", messages: &["feat: base hardening role", "feat: node exporter role", "fix: idempotent sysctl"], branches: &[], dirty: false },
    Theme { path: "data/etl-pipelines", ext: "py", messages: &["feat: daily charges export", "feat: refunds export", "fix: timezone of partitions", "perf: read parquet in chunks", "feat: freshness alerts"], branches: &["feature/backfill-cli"], dirty: true },
    Theme { path: "data/warehouse-models", ext: "sql", messages: &["feat: revenue by day", "feat: refund rate", "fix: exclude test merchants"], branches: &["feature/cohorts"], dirty: false },
    Theme { path: "docs/handbook", ext: "md", messages: &["docs: how we review", "docs: on-call rotation", "docs: incident template", "docs: release checklist"], branches: &[], dirty: false },
    Theme { path: "tools/release-bot", ext: "sh", messages: &["feat: tag and changelog", "fix: skip draft releases", "feat: post to chat"], branches: &["feature/dry-run"], dirty: false },
];

/// A line of source in the language of the file: the nth step of the history.
fn line(ext: &str, n: usize) -> String {
    match ext {
        "go" => format!("func step{n}() int {{ return {n} }}\n"),
        "ts" => format!("export const step{n} = (): number => {n};\n"),
        "py" => format!("def step{n}():\n    return {n}\n\n"),
        "hcl" => format!("variable \"step_{n}\" {{\n  default = {n}\n}}\n\n"),
        "yaml" => format!("- name: step {n}\n  value: {n}\n"),
        "sql" => format!("SELECT {n} AS step_{n};\n"),
        "md" => format!("- step {n}\n"),
        _ => format!("echo \"step {n}\"\n"),
    }
}

fn file_name(ext: &str) -> String {
    match ext {
        "md" => "README.md".to_string(),
        other => format!("main.{other}"),
    }
}

fn build_one(git: &Git, work: &Path, seeds: &Path, theme: &Theme, rng: &mut Rng) {
    let seed = seeds.join(theme.path.replace('/', "-"));
    fs::create_dir_all(&seed).expect("seed");
    git.run(&seed, &["init", "-q", "-b", "main"]);
    let name = file_name(theme.ext);
    let mut text = String::new();
    let count = theme.messages.len() as i64;
    for (n, message) in theme.messages.iter().enumerate() {
        text.push_str(&line(theme.ext, n + 1));
        write(&seed, &name, &text);
        let days_ago = 380 - (n as i64) * (350 / count) + rng.below(9) as i64;
        let who = &PEOPLE[rng.below(PEOPLE.len())];
        git.commit(&seed, who, days_ago, 9 + rng.below(8) as i64, message);
    }
    for (k, branch) in theme.branches.iter().enumerate() {
        git.run(&seed, &["switch", "-q", "-c", branch]);
        text.push_str(&line(theme.ext, 100 + k));
        write(&seed, &name, &text);
        let who = &PEOPLE[rng.below(PEOPLE.len())];
        git.commit(&seed, who, 12 - k as i64 * 4, 14, &format!("wip: {}", branch.rsplit('/').next().unwrap_or(branch)));
        git.run(&seed, &["switch", "-q", "main"]);
    }
    let root = work.join(theme.path);
    let with_tree = theme.branches.len().saturating_sub(1);
    let mut trees = vec!["main"];
    trees.extend(theme.branches.iter().take(with_tree).copied());
    layout(git, &seed, &root, &trees, &["main"]);
    if theme.dirty {
        let main = root.join("main");
        let mut changed = fs::read_to_string(main.join(&name)).expect("read");
        changed.push_str(&line(theme.ext, 200));
        write(&main, &name, &changed);
    }
}

/// A repository that arrived as a folder from a partner: its own config and hook can run
/// programs, so HydraMerge does not read it until the user trusts it.
fn partner_sdk(git: &Git, work: &Path, seeds: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let seed = seeds.join("partner-sdk");
    fs::create_dir_all(&seed).expect("seed");
    git.run(&seed, &["init", "-q", "-b", "main"]);
    let steps = ["feat: client skeleton", "feat: signed requests", "fix: retry on 429"];
    let mut text = String::new();
    for (n, message) in steps.iter().enumerate() {
        text.push_str(&line("go", n + 1));
        write(&seed, "client.go", &text);
        git.commit(&seed, &PEOPLE[8 + n], 200 - n as i64 * 40, 12, message);
    }
    let root = work.join("vendor/partner-sdk");
    layout(git, &seed, &root, &["main"], &["main"]);
    let bare = format!("--git-dir={}", root.join(".bare").display());
    for (key, value) in [
        ("core.sshCommand", "ssh -i ~/.ssh/partner_ed25519 -o IdentitiesOnly=yes"),
        ("filter.vault.smudge", "vault-decrypt %f"),
        ("filter.vault.clean", "vault-encrypt %f"),
    ] {
        git.run(&root, &[&bare, "config", key, value]);
    }
    let hook = root.join(".bare/hooks/pre-commit");
    fs::write(&hook, "#!/bin/sh\nexec ./scripts/lint-and-sign.sh \"$@\"\n").expect("hook");
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).expect("chmod");
}

/// Builds every repository except `payments-api` under `work`.
pub fn build(git: &Git, work: &Path, seeds: &Path) {
    let mut rng = Rng::new(2026);
    for theme in &THEMES {
        build_one(git, work, seeds, theme, &mut rng);
    }
    partner_sdk(git, work, seeds);
}
