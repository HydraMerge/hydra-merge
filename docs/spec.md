# HydraMerge — контракт реализации (Rust, нативное приложение macOS)

Версия документа: 2.0, 2026-10-08. Заменяет версию 1.0 (Python `http.server` + браузер), которую пользователь отклонил.
Основа: раскладка «bare + папка на ветку».
Документ описывает, что нужно сделать. Код приложения здесь не пишется.

---

## 1. Цель

`hydra-merge` — нативное десктопное приложение macOS на Rust в корне этого репозитория. Одно окно с заголовком `HydraMerge`.
Оно показывает все репозитории с раскладкой «bare + папка на ветку», открывает выбранный worktree в редакторе,
создаёт новый worktree для свободной локальной ветки и удаляет чистый worktree.

`hydra-merge` показывает историю, diff и commit, переключает между worktree и создаёт их.

Целевая раскладка:

```
<repo_root>/.bare/          bare git dir
<repo_root>/<branch>/       worktree, HEAD которого — эта ветка
```

`<repo_root>` — каталог, в котором лежит `.bare`. Для ветки `feature/foo` путь worktree — `<repo_root>/feature/foo`
(каждый `/` в имени ветки — отдельный сегмент пути).

Окружение: macOS, Rust stable (edition 2021), git ≥ 2.20 (на этом Mac 2.54.0).

### 1.1 Зафиксированный стек

| Пункт | Решение |
|---|---|
| Язык | Rust, `edition = "2021"` |
| Имя бинарника | `hydra-merge` |
| GUI | только `eframe` + `egui` (egui берётся как `eframe::egui`). Tauri, GTK, iced, Swift, webview, `rfd` запрещены |
| Git | только `std::process::Command` с argv. `git2`/libgit2/gitoxide запрещены |
| Сеть | приложение во время работы не открывает сокетов и не делает HTTP-запросов. HTTP-сервера нет |
| Заголовок окна | `HydraMerge` |
| Темы | две `egui::Visuals` (dark, light), токены из §8.3 |

### 1.2 Удаление Python-реализации

Реализатор **обязан удалить**:

- `hydra_merge.py`
- каталог `static/` целиком (`index.html`, `app.js`, `app.css`)
- `tests/test_hydra_merge.py` и `tests/__pycache__/`
- `__pycache__/` в корне
- Python-части `README.md`: разделы про `python3 hydra_merge.py`, `--port`, `127.0.0.1:8765`, HTTP-проверки (`Host`, `Origin`,
  `Content-Type`, лимит тела), `python3 -m unittest`. README переписывается под Rust по §10.

`docs/` (кроме этого файла) не меняется. `/Applications` и `~/repos` не меняются: приложение туда не пишет,
установка `.app` в `/Applications` не входит в объём работ.

---

## 2. Опорные возможности

Окно показывает репозитории с раскладкой bare + папка на ветку, дерево веток, историю, diff, светлую и тёмную тему. Внешний редактор открывает папку ветки и не управляет worktree.

## 3. Чего не хватает

1. **Checkout ветки, занятой другим worktree, не работает.** Git отвечает
   `fatal: '<branch>' is already used by worktree at '<path>'`. Нужно открыть папку того worktree, а не делать checkout.
2. **Обычный клиент не создаёт раскладку `.bare/` + `<branch>/`.**
3. **Он не знает о `<repo_root>`.** Родительский каталог даёт `fatal: this operation must be run in a work tree`,
   `.bare` открывается как bare-репозиторий без рабочей копии. Нужна точка входа, которая открывает папку ветки.
4. **Нет общего списка всех репозиториев с этой раскладкой** под `~/repos`, с флагами «грязный» и
   «путь не совпадает с раскладкой».

`hydra-merge` закрывает эти пункты и больше ничего не делает. Он не конвертирует обычные клоны, не делает
checkout/reset/clean/push/commit, не меняет настройки редактора.

---

## 4. Состав крейта

Все пути относительно корня репозитория.

| Файл | Назначение | Ограничение |
|---|---|---|
| `Cargo.toml` | манифест (§4.1) | — |
| `Cargo.lock` | коммитится (это приложение) | — |
| `.gitignore` | `/target` | — |
| `src/main.rs` | точка входа: `Config::from_env`, `eframe::run_native` | ≤ 60 строк |
| `src/lib.rs` | `pub mod app; diff; diff_view; discover; git; highlight; model; theme;` — нужен, чтобы `tests/` видели API | только объявления модулей |
| `src/model.rs` | `Config`, `Repo`, `Worktree`, `Layout`, `ErrorCode`, `AppError` | ≤ 800 строк |
| `src/git/` | `mod.rs` (реэкспорт), `process.rs` (`run_git`, тайм-ауты), `worktrees.rs` (porcelain, list/create/remove), `refs.rs` (ветки, remotes, теги, stash, submodule, удаление удалённой ветки), `changes.rs` (status, log, patch, stage/commit/push), `editor.rs` (открытие в Sublime Text) | каждый файл ≤ 800 строк, функции ≤ 50 строк |
| `src/discover.rs` | `discover_repos`, `is_bare_repo`, `resolve_*`, `layout_path_for` | ≤ 800 строк |
| `src/theme.rs` | `Palette`, `LIGHT`, `DARK`, `visuals()`, `ThemeChoice`, загрузка/сохранение `theme.txt` | ≤ 800 строк |
| `src/diff.rs`, `src/diff_view.rs` | разбор `git show -p` в строки и их отрисовка (только видимые строки) | ≤ 800 строк |
| `src/highlight/` | построчная подсветка синтаксиса (`mod.rs` — лексер, `langs.rs` — таблица языков) | ≤ 800 строк |
| `src/app/` | `WorktreeBarApp: eframe::App`: `mod.rs` (состояние), `jobs.rs`, `actions.rs`, `top.rs`, `locations.rs`, `tree.rs`, `central.rs`, `history.rs`, `dialogs.rs` | каждый файл ≤ 800 строк |
| `tests/git_ops.rs` | интеграционные тесты на временных git-репозиториях | §9 |
| `tests/common/mod.rs` | фикстуры (`make_bare_layout`, `make_plain_clone`, `fake_smerge`, `test_config`) | — |

Юнит-тесты разбора (porcelain, имена веток, токены темы) допускаются в `#[cfg(test)] mod tests` внутри `src/git.rs` и `src/theme.rs`.
Изображения и ресурсы из `HydraMerge.app` не копируются. Иконки — символы Unicode из встроенных шрифтов egui.

### 4.1 `Cargo.toml`

```toml
[package]
name = "hydra-merge"
version = "0.1.0"
edition = "2021"
publish = false

[lib]
name = "hydra_merge"
path = "src/lib.rs"

[[bin]]
name = "hydra-merge"
path = "src/main.rs"

[dependencies]
eframe = { version = "=<точная версия ≥ 0.29>", default-features = false, features = ["default_fonts", "glow", "accesskit"] }

[dev-dependencies]
tempfile = "3"
```

- Других runtime-зависимостей нет (`dirs`, `thiserror`, `serde`, `anyhow`, `rfd`, `git2` не добавляются).
- Версия `eframe` фиксируется точно (`=X.Y.Z`) и должна быть ≥ 0.29: нужны `egui::Context::system_theme()`,
  `Context::set_theme(ThemePreference)`, `Context::set_visuals_of(Theme, Visuals)`, `Options::fallback_theme`.
  Если в выбранной версии поле `WidgetVisuals` называется `rounding` (≤ 0.30), а не `corner_radius` (≥ 0.31), используется то,
  что есть; в отчёте указывается выбранная версия.
- Фича `persistence` не включается: eframe ничего не сохраняет сам, единственный файл состояния — `theme.txt` (§8.2).

### 4.2 `src/main.rs`

```rust
fn main() -> eframe::Result<()>
```

1. `let config = Config::from_env();`
2. `eframe::NativeOptions { viewport: ViewportBuilder::default().with_title("hydra-merge").with_inner_size([960.0, 600.0]).with_min_inner_size([640.0, 400.0]), ..Default::default() }`
3. `eframe::run_native("hydra-merge", options, Box::new(|cc| Ok(Box::new(WorktreeBarApp::new(cc, config)))))`

Других флагов командной строки нет. Окно открывается сразу; discovery выполняется в фоне (§8.4).

---

## 5. Модель и discovery

### 5.1 Типы (`src/model.rs`)

Все типы данных неизменяемые по смыслу: после построения поля не меняются, новое состояние — новый объект.
Снимок списка в приложении хранится как `Arc<[Repo]>` и целиком заменяется после скана.

```rust
pub const BARE_DIRNAME: &str = ".bare";
pub const DEFAULT_ROOTS: [&str; 1] = ["~/repos"];
pub const ENV_ROOTS: &str = "HYDRA_MERGE_ROOTS";      // пути через ':'
pub const ENV_SMERGE: &str = "HYDRA_MERGE_SMERGE";
pub const DEFAULT_SMERGE: &str = "/Applications/Sublime Text.app/Contents/SharedSupport/bin/subl";
pub const MAX_DEPTH: usize = 4;
pub const GIT_TIMEOUT: Duration = Duration::from_secs(15);
pub const SMERGE_TIMEOUT: Duration = Duration::from_secs(10);
pub const SKIP_DIRS: [&str; 3] = [".bare", ".git", "node_modules"];

#[derive(Clone, Debug)]
pub struct Config {
    pub roots: Vec<PathBuf>,              // canonicalized, существующие, без дублей, порядок сохранён
    pub smerge: PathBuf,
    pub git_env: Vec<(OsString, OsString)>, // доп. переменные для каждого вызова git (тесты: GIT_CONFIG_GLOBAL и т.п.)
}

impl Config {
    pub fn from_env() -> Config;   // читает HOME, HYDRA_MERGE_ROOTS, HYDRA_MERGE_SMERGE и вызывает from_values
    pub fn from_values(home: &Path, roots_var: Option<&OsStr>, smerge_var: Option<&OsStr>) -> Config; // чистая функция
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout { Bare, Plain }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Worktree {
    pub path: PathBuf,              // canonicalized (если каталог есть), иначе путь из porcelain
    pub branch: Option<String>,     // "feature/foo"; None для detached и bare
    pub head: Option<String>,       // 7 hex-символов; None для bare
    pub bare: bool,                 // запись .bare из `worktree list`
    pub detached: bool,
    pub locked: bool,
    pub prunable: bool,
    pub dirty: Option<bool>,        // None для bare и prunable
    pub layout_path: Option<PathBuf>,
    pub layout_correct: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Repo {
    pub root: PathBuf,              // canonicalized <repo_root> (для plain — toplevel клона)
    pub name: String,               // путь root относительно discovery root, напр. "group/repo"
    pub layout: Layout,
    pub git_dir: PathBuf,           // <root>/.bare для Bare; `rev-parse --absolute-git-dir` для Plain
    pub worktrees: Vec<Worktree>,
}
```

`Config::from_values`:
- `roots_var` задан → разбить по `:`, пустые элементы пропустить; иначе `DEFAULT_ROOTS`.
- `~` и `~/…` раскрываются через `home`. Затем `fs::canonicalize`; несуществующие пути молча пропускаются; дубликаты (после canonicalize) удаляются, порядок не меняется.
- `smerge_var` задан и не пуст → он; иначе `DEFAULT_SMERGE`.
- `git_env` пустой.

Тесты не трогают переменные окружения процесса (тесты Rust идут параллельно в одном процессе): они строят `Config` напрямую.

### 5.2 Ошибки (`src/model.rs`)

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorCode {
    InvalidBranch, NotARepo, OutsideRoots, UnknownRepo, UnknownWorktree,
    BareNotOpenable, ParentNotOpenable, BranchNotFound, BranchInUse, DestExists, DestNested,
    DirtyWorktree, LockedWorktree, MainWorktree, PlainLayout, GitFailed, GitTimeout, MergeNotFound, Io,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,           // короткий текст для пользователя
    pub detail: Option<String>,    // stderr git/smerge, обрезанный до 2000 символов
}
impl std::fmt::Display for AppError { … }   // "<message>"
impl std::error::Error for AppError {}
pub type AppResult<T> = Result<T, AppError>;
```

Каждая публичная функция `git.rs`/`discover.rs` возвращает `AppResult`. `unwrap()`/`expect()` на данных из ФС или git запрещены;
паника допускается только в тестах. Каждая ошибка показывается в окне (§8.1) и дублируется в stderr строкой `error: <code:?>: <message>`.

| `ErrorCode` | Когда | Сообщение (шаблон) |
|---|---|---|
| `InvalidBranch` | §5.5 | `Invalid branch name '<name>': <причина>` |
| `NotARepo` | Open path… указывает не на репозиторий | `Not a git repository: <path>` |
| `OutsideRoots` | путь не под discovery root | `Path is outside discovery roots: <path>` |
| `UnknownRepo` | root не совпадает с известным репо | `Unknown repository: <path>` |
| `UnknownWorktree` | путь не совпадает с worktree из свежего списка | `Unknown worktree: <path>` |
| `BareNotOpenable` | путь — `.bare` или внутри него, либо запись `bare: true` | `The .bare directory cannot be opened or removed` |
| `ParentNotOpenable` | путь равен `repo_root` | `The repository root is not a worktree: <path>` |
| `BranchNotFound` | ветки нет в `refs/heads` | `Local branch not found: <branch>` |
| `BranchInUse` | ветка уже в worktree | `Branch '<branch>' is already used by worktree at <path>` |
| `DestExists` | путь назначения существует | `Destination already exists: <path>` |
| `DestNested` | назначение внутри worktree или содержит worktree | `Destination overlaps worktree at <path>` |
| `DirtyWorktree` | удаление грязного worktree | `Worktree has changes, not removed: <path>` |
| `LockedWorktree` | удаление locked | `Worktree is locked: <path>` |
| `MainWorktree` | удаление main worktree plain-клона | `Main worktree cannot be removed: <path>` |
| `PlainLayout` | create/remove для `Layout::Plain` | `Create/remove is only available for .bare layout` |
| `GitFailed` | неожиданный ненулевой код git/smerge | `git <подкоманда> failed (exit <code>)` |
| `GitTimeout` | превышен `GIT_TIMEOUT`/`SMERGE_TIMEOUT` | `git <подкоманда> timed out` |
| `MergeNotFound` | smerge нет или не исполняемый | `HydraMerge CLI not found: <path>` |
| `Io` | ошибка ФС/запуска процесса | `<операция>: <io::Error>` |

### 5.3 Публичные функции `src/discover.rs`

| Функция | Что делает |
|---|---|
| `pub fn discover_repos(config: &Config) -> AppResult<Vec<Repo>>` | обход по §5.4, для каждого hit вызывает `git::list_worktrees` |
| `pub fn is_bare_repo(config: &Config, bare_path: &Path) -> bool` | §7.1 |
| `pub fn layout_path_for(repo_root: &Path, branch: &str) -> AppResult<PathBuf>` | §5.5 |
| `pub fn ensure_under_roots(config: &Config, path: &Path) -> AppResult<PathBuf>` | canonicalize + проверка `starts_with` одного из `config.roots`; иначе `OutsideRoots` |
| `pub fn resolve_repo(config: &Config, root: &Path) -> AppResult<Repo>` | свежий скан; `root` точно равен `Repo::root`; иначе `UnknownRepo` |
| `pub fn resolve_worktree(config: &Config, path: &Path) -> AppResult<(Repo, Worktree)>` | проверки §6 |
| `pub fn open_path(config: &Config, path: &Path) -> AppResult<Repo>` | ручное добавление (§5.6) |

### 5.4 Обход discovery roots

1. Корни — `config.roots` по порядку (по умолчанию `~/repos`).
2. Обход в ширину через `fs::read_dir` до глубины `MAX_DEPTH = 4`, корень — глубина 0.
   Ожидаемые находки: `~/repos/<group>/<repo>` (глубина 2) и `~/repos/<repo>` (глубина 1).
3. Для каждого каталога `D` (включая корень):
   - если `D/.bare` существует (каталог **или** файл) и `is_bare_repo(D/.bare)` — hit `Layout::Bare`. Внутрь `D` обход **не спускается**:
     папки веток не сканируются как отдельные репозитории;
   - иначе если `D/.git` — каталог, это обычный репозиторий: hit `Layout::Plain`, обход внутрь не спускается;
   - иначе если `D/.git` — файл (связанный worktree), это не отдельный репозиторий и внутрь обход не спускается;
   - иначе спуск в дочерние каталоги, кроме: имён из `SKIP_DIRS`, всех имён, начинающихся с `.`, и символических ссылок
     (`DirEntry::file_type()?.is_symlink()` → пропуск).
4. Дедупликация по canonicalized `D`. Если один repo root виден из нескольких корней, он попадает один раз, с `name`
   относительно **первого** корня, где он найден.
5. `io::Error` при чтении каталога пишется в stderr `warn: cannot scan <path>: <err>` и пропускается (скан не прерывается).
6. Результат сортируется по `name.to_lowercase()`.

Discovery только читает и ничего не пишет.

### 5.5 Имя ветки → путь

`git::validate_branch_name(config, name) -> AppResult<()>`. Проверки по порядку, первая сработавшая даёт `InvalidBranch`:

1. `name` непустая, ≤ 255 байт, без `\0`, `\\`, `\n`, `\r`.
2. Не начинается с `/` и `-`, не заканчивается на `/` (абсолютные пути и опции отсекаются).
3. `name != ".bare"`.
4. Каждый сегмент после `split('/')` непустой, не равен `.` и `..`, не начинается с `.` (отсекает `..`, `.bare`, `.git` и скрытые каталоги на любом уровне).
5. `git check-ref-format --branch <name>` завершается с кодом 0 (§7.2).

`layout_path_for(repo_root, branch)`: проверки 1–4 (без git), затем `repo_root.join(seg1).join(seg2)…`. Дополнительно
результат должен `starts_with(repo_root)` и не быть равен `repo_root`. Слэши в имени ветки — сегменты пути, а не разделитель внутри имени файла.

`layout_correct = canonical(wt.path) == layout_path_for(repo.root, wt.branch)`. Если ветки нет или имя не проходит проверку —
`layout_path = None`, `layout_correct = false`. Для `Layout::Plain` всегда `layout_path = None`, `layout_correct = false`.

### 5.6 Обычные клоны (`Layout::Plain`)

Добавляются вручную через «Open path…» (поле ввода пути в окне; системный диалог файлов не используется).

- `path` проходит `ensure_under_roots`.
- Если `path/.bare` — bare-репозиторий, результат тот же, что у hit `Bare`.
- Иначе `git -C <path> rev-parse --show-toplevel`; ненулевой код → `NotARepo`.
- Toplevel становится `Repo { layout: Plain }`, worktree — из `git -C <toplevel> worktree list --porcelain`.
- Для plain доступны только просмотр и Open. Create и Remove → `PlainLayout`. Конвертации нет.
- Список добавленных путей хранится в состоянии приложения (не на диске) и пересканируется вместе со снимком.

### 5.7 Запись `.bare` в списке worktree

`git --git-dir=<root>/.bare worktree list --porcelain` первой записью выдаёт сам `.bare` со строкой `bare`. Эта запись:
- попадает в `Repo::worktrees` с `bare: true` и показывается в списке серым как «.bare (bare)»;
- **не выбирается** в левой панели и **не предлагается** для Open/Remove: строка неактивна, кнопки не рисуются;
  `open_in_merge`/`remove_worktree` для неё возвращают `BareNotOpenable` (защита в глубину);
- не проверяется `status`, `dirty = None`.

---

## 6. Проверки пути перед каждым действием

`resolve_worktree(config, path)` выполняет по порядку:

1. `ensure_under_roots` → иначе `OutsideRoots`.
2. Если путь равен `<root>/.bare` любого известного репо или лежит внутри него → `BareNotOpenable`.
3. Если путь равен `root` известного репо → `ParentNotOpenable`.
4. Путь должен **точно** совпасть с `Worktree::path` в свежем `list_worktrees` одного из известных репо (скан + добавленные plain).
   Иначе `UnknownWorktree`.
5. Если найденная запись имеет `bare: true` → `BareNotOpenable`.

Для Create значение `repo` проходит `ensure_under_roots` и `resolve_repo` (иначе `UnknownRepo`).
Приложение никогда не строит путь для действия из свободного ввода, кроме «Open path…», который тоже проходит эти проверки.

---

## 7. Git argv для каждого действия (`src/git.rs`)

### 7.0 Общие правила

```rust
pub struct GitOutput { pub status: Option<i32>, pub stdout: String, pub stderr: String }

pub fn run_git(config: &Config, args: &[&OsStr], cwd: &Path) -> AppResult<GitOutput>;
fn run_with_timeout(cmd: Command, timeout: Duration) -> AppResult<GitOutput>;  // общий для git и smerge
```

- `run_git` — единственное место, где запускается git. `Command::new("git").args(args).current_dir(cwd)`,
  `stdin(Stdio::null())`, `stdout/stderr(Stdio::piped())`.
- Никаких `sh -c`, `bash`, `osascript`, сборки командной строки строкой. Только argv.
- Окружение: наследуется + `GIT_TERMINAL_PROMPT=0`, `GIT_OPTIONAL_LOCKS=0`, `LC_ALL=C`, плюс `config.git_env`.
- Тайм-аут: `spawn()`, stdout и stderr читаются в двух `std::thread` (чтобы не было взаимной блокировки на полном pipe),
  родитель опрашивает `child.try_wait()` каждые 20 мс до `GIT_TIMEOUT`. По истечении — `child.kill()`, `child.wait()`, `GitTimeout`.
- Ошибка `spawn` → `Io` (`failed to run git: <err>`).
- Пользовательские значения (ветка, пути) идут **после** опций. Ветка не может начинаться с `-` (§5.5), пути абсолютные.
- Запрещённые подкоманды и флаги (не встречаются как строковые литералы в `src/git.rs`, `src/discover.rs`, `src/model.rs`): `checkout`, `switch`, `reset`, `clean`,
  `merge`, `rebase`, `branch -d/-D`, `--force`, `-f`, `worktree prune`. Удаление `.bare`,
  `fs::remove_dir_all` и `fs::remove_file` в `src/` запрещены. Это проверяет тест `source_has_no_forbidden_git` (§9).
- Разрешённые записи в выбранный worktree: `add` (`-- <path>` или `--all`), `restore --staged`, `restore --worktree --source=HEAD` (только отслеживаемый файл, с подтверждением в окне), `commit -m`, `fetch`, `pull --no-edit`, `push`. Пустое сообщение и путь с `..`, абсолютный или начинающийся с `-` отклоняются до запуска git. Fetch, pull и push используют `NETWORK_TIMEOUT` (120 с); остальные команды — `GIT_TIMEOUT`. `--force` не передаётся.

### 7.1 Проверка bare — `discover::is_bare_repo`

```
["git", "--git-dir=<root>/.bare", "rev-parse", "--is-bare-repository"]        cwd=<root>
```
Hit, если код 0 и `stdout.trim() == "true"`. Аргумент `--git-dir=…` собирается как один `OsString` (`"--git-dir="` + путь).

### 7.2 Валидация имени ветки

```
["git", "check-ref-format", "--branch", "<branch>"]                          cwd=<root>
```

### 7.3 Список worktree — `pub fn list_worktrees(config, repo_root: &Path, git_dir: &Path, layout: Layout) -> AppResult<Vec<Worktree>>`

```
Bare:   ["git", "--git-dir=<root>/.bare", "worktree", "list", "--porcelain"]   cwd=<root>
Plain:  ["git", "-C", "<toplevel>", "worktree", "list", "--porcelain"]       cwd=<toplevel>
```

Разбор — чистая функция `pub fn parse_worktree_porcelain(stdout: &str) -> Vec<RawWorktree>` (юнит-тесты без git).
Записи разделены пустой строкой. Распознаются `worktree <path>`, `HEAD <sha>`, `branch refs/heads/<name>`, `bare`, `detached`,
`locked[ <reason>]`, `prunable[ <reason>]`. Неизвестные строки игнорируются. `head = sha[..7]`. Префикс `refs/heads/` удаляется.
Путь canonicalize, если каталог существует.

### 7.4 Dirty — `pub fn is_dirty(config, wt_path: &Path) -> AppResult<bool>`

Для записей с `bare == false`, `prunable == false` и существующим каталогом:
```
["git", "-C", "<wt_path>", "status", "--porcelain=v1", "--untracked-files=normal"]   cwd=<wt_path>
```
Непустой stdout → `true`. Untracked-файлы тоже делают worktree грязным (так же считает `git worktree remove`).

### 7.5 Локальные ветки — `pub fn branches(config, repo: &Repo) -> AppResult<BranchList>`

```rust
pub struct BranchList { pub available: Vec<String>, pub used: Vec<(String, PathBuf)> }
```
```
["git", "--git-dir=<root>/.bare", "for-each-ref", "--format=%(refname)", "refs/heads/"]   cwd=<root>
```
Префикс `refs/heads/` удаляется; `%(refname:short)` не используется. `available` — ветки, которых нет ни в одной записи `branch`
списка worktree, отсортированные. `used` — занятые ветки с путём worktree; по ним UI предлагает Open вместо Create.

Проверка существования:
```
["git", "--git-dir=<root>/.bare", "rev-parse", "--verify", "--quiet", "refs/heads/<branch>"]   cwd=<root>
```
Ненулевой код → `BranchNotFound`.

### 7.6 Open — `pub fn open_in_merge(config, path: &Path) -> AppResult<PathBuf>`

```
[<config.smerge>, "<wt_path>"]          cwd=<wt_path>
```
- Сначала `resolve_worktree` (§6). Только после успешной проверки запускается smerge.
- `<config.smerge>` должен существовать и иметь бит исполнения (`metadata.permissions().mode() & 0o111 != 0`), иначе `MergeNotFound`.
- Аргумент один: путь worktree. Никогда не передаётся `.bare`, `repo_root` или запись `bare: true`.
- Тайм-аут `SMERGE_TIMEOUT` (через `run_with_timeout`). Ненулевой код → `GitFailed` с `detail` из stderr.
- Возвращает открытый путь.

### 7.7 Create worktree — `pub fn create_worktree(config, repo_root: &Path, branch: &str) -> AppResult<Worktree>`

Путь назначения вычисляет приложение, пользователь его не вводит.

1. `resolve_repo`. `Layout::Plain` → `PlainLayout`.
2. `validate_branch_name` (§5.5), затем проверка существования (§7.5).
3. Свежий `list_worktrees`. Запись с `branch == Some(branch)` → `BranchInUse` с её путём в message. git не вызывается.
4. `dest = layout_path_for(root, branch)`. `dest.symlink_metadata().is_ok()` → `DestExists`. git не вызывается.
5. `dest` внутри пути существующего не-bare worktree или существующий worktree внутри `dest` → `DestNested`
   (пример: есть worktree `feature`, создаётся `feature/foo`).
6. Команда:
   ```
   ["git", "--git-dir=<root>/.bare", "worktree", "add", "<dest>", "<branch>"]   cwd=<root>
   ```
   Без `-b`, `-B`, `--force`, `--detach`. Промежуточные каталоги (`feature/`) git создаёт сам.
7. Ненулевой код: stderr содержит `is already used by worktree` или `is already checked out at` → `BranchInUse` (гонка с п.3);
   содержит `already exists` → `DestExists`; иначе `GitFailed`.
8. Возвращает новый `Worktree` из свежего `list_worktrees`.

### 7.8 Remove worktree — `pub fn remove_worktree(config, path: &Path) -> AppResult<PathBuf>`

1. `resolve_worktree` (§6). `Layout::Plain` → `PlainLayout`; путь равен main worktree plain-репо → `MainWorktree`.
2. `locked` → `LockedWorktree`.
3. `is_dirty` → `true` → `DirtyWorktree`. Удаление не выполняется.
4. Команда:
   ```
   ["git", "--git-dir=<root>/.bare", "worktree", "remove", "<wt_path>"]   cwd=<root>
   ```
   `--force` и `-f` не передаются никогда. Если git всё равно отказал (worktree стал грязным между проверкой и удалением) →
   `DirtyWorktree` с `detail` из stderr.
5. Каталог `.bare`, ветка и пустые родительские каталоги (`feature/`) не удаляются. Приложение само ничего не удаляет из ФС.

---

## 8. UI (`src/app.rs`)

### 8.1 Раскладка окна

```
┌───────────────────────────────────────────────────────────────────┐
│ TopBottomPanel "top": HydraMerge  [⟳ Rescan] [Open path…] [◐ Dark/Light] │
├──────────────────────┬────────────────────────────────────────────┤
│ SidePanel::left      │ CentralPanel: выбранный worktree           │
│ "LOCATIONS"          │ Branch:  test                              │
│ ▾ infra/dash   bare  │ Path:    /…/infra/dash/test                │
│    master  a1b2c3d   │ Layout:  ✓ matches <root>/test             │
│  ● test    9f8e7d6 ● │ HEAD:    9f8e7d6   Status: dirty           │
│    .bare   (bare)    │ [Open] [Remove worktree]  │
│ ▾ legacy-clone plain │ ── Create worktree ──                      │
│    main    0011aab   │ Branch: [ComboBox: available ▾] [Create]   │
│                      │ Used: feature/x → /…/feature/x [Open]      │
├──────────────────────┴────────────────────────────────────────────┤
│ TopBottomPanel "bottom": ошибка (message + ▸ detail, [×]) / статус  │
└───────────────────────────────────────────────────────────────────┘
```

- **Левая панель — Locations** (`SidePanel::left("locations")`, `resizable(true)`, ширина по умолчанию 280, мин. 200,
  `Frame` с `palette.sidebar_bg`). Дерево: репозиторий (`CollapsingHeader`, текст `name`, бейдж `bare`/`plain`), под ним worktree.
  Строка worktree: ветка или `(detached)`, short HEAD моноширинным шрифтом, точка `●` цвета `dirty` если `dirty == Some(true)`,
  `⚠` если `layout_correct == false`. Полный путь — во всплывающей подсказке (`on_hover_text`).
- Строка `bare: true` рисуется `palette.row_disabled_fg` через `ui.add_enabled(false, …)`; выбрать её нельзя.
- **Правая панель** (`CentralPanel`) — выбранный worktree: branch, path, layout_path, `layout_correct` (✓ цвета `clean` или ⚠ с ожидаемым
  путём), HEAD, dirty/clean, флаги locked/prunable. Действия:
  - **Open** (`Open`) → `git::open_in_merge`.
  - **Remove** (`Remove worktree`, цвет `button_danger_*`) → модальное окно подтверждения с полным путём (Cancel / Remove) →
    `git::remove_worktree`. Кнопка неактивна при `dirty != Some(false)`, `locked`, `Layout::Plain`. Функция всё равно проверяет это сама.
  - **Create** (цвет `button_primary_*`) → `ComboBox` из `git::branches(...).available`; ниже список `used` с кнопкой Open для каждой
    (ответ на «checkout ветки, занятой другим worktree»). Секция скрыта для `Layout::Plain`.
  - Если ничего не выбрано — текст «Select a worktree in Locations».
- **Ошибки показываются в окне**, а не только в stderr: нижняя панель с фоном `error_bg`, текстом `error_fg`, полосой слева `error_accent`,
  `message` и сворачиваемым `detail` (`CollapsingHeader "Details"`), кнопка `×` закрывает. Последняя ошибка заменяет предыдущую.
  Без ошибки панель показывает статус (`Scanned N repositories`, `Opened <path>`, `Created <path>`, `Removed <path>`) цветом `text_muted`.
- Клавиатура: ↑/↓ — по строкам Locations, `Enter` — Open, `Cmd+R` — Rescan. Обрабатываются, только когда поле ввода не в фокусе.
- Типографика: шрифты egui по умолчанию; `TextStyle::Body` 12 pt, `Small` 11 pt, `Heading` 13 pt, `Monospace` 12 pt, `Button` 12 pt.

### 8.2 Темы

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeChoice { Dark, Light }

pub fn palette(choice: ThemeChoice) -> &'static Palette;
pub fn visuals(choice: ThemeChoice) -> egui::Visuals;          // строится из Palette по §8.3
pub fn apply_fonts(ctx: &egui::Context);                        // размеры из §8.1
pub fn config_dir(home: &Path) -> PathBuf;                      // <home>/Library/Application Support/hydra-merge
pub fn theme_file(dir: &Path) -> PathBuf;                       // <dir>/theme.txt
pub fn load_theme_choice(dir: &Path) -> Option<ThemeChoice>;    // "dark"/"light" после trim; иначе None
pub fn save_theme_choice(dir: &Path, choice: ThemeChoice) -> AppResult<()>; // create_dir_all + запись "dark\n"/"light\n"
pub fn parse_theme(s: &str) -> Option<ThemeChoice>;
```

- Файл: `~/Library/Application Support/hydra-merge/theme.txt`, содержимое `dark` или `light` (перевод строки допускается).
  `home` берётся из `$HOME`; если `HOME` не задан, тема не сохраняется (ошибка показывается в окне один раз).
  Любое другое содержимое или отсутствие файла = выбора нет. Других файлов приложение не пишет.
- При старте (`WorktreeBarApp::new`):
  1. `ctx.set_visuals_of(Theme::Dark, visuals(Dark))`, `ctx.set_visuals_of(Theme::Light, visuals(Light))`, `apply_fonts`.
  2. `ctx.options_mut(|o| o.fallback_theme = Theme::Dark)`.
  3. Файл дал выбор → `ctx.set_theme(ThemePreference::Dark|Light)`. Иначе `ctx.set_theme(ThemePreference::System)`:
     egui следует за внешним видом системы, если eframe сообщает его (`ctx.system_theme()` = `Some`), иначе применяется `fallback_theme` = dark.
- Кнопка **◐** в верхней панели (и пункт меню View → Toggle theme): берёт текущую тему `ctx.theme()`, переключает на противоположную,
  вызывает `ctx.set_theme(ThemePreference::<новая>)` и `save_theme_choice`. Ошибка записи показывается в окне, тема всё равно меняется на сессию.
  Подпись кнопки — тема, на которую будет переключение (`◐ Light` в тёмной теме, `◐ Dark` в светлой).
- Цвета в `app.rs` берутся только из `theme::palette(...)` или `ctx.style().visuals`. Литералы `Color32::from_*` вне `theme.rs` запрещены.

### 8.3 Таблица токенов темы

Значения палитры заданы в hex. HSL пересчитан в hex,
альфа смешана с фоном под элементом. В `theme.rs` токены — поля `pub struct Palette { pub bg: Color32, … }`,
две константы `pub const LIGHT: Palette` и `pub const DARK: Palette`, цвета задаются `const fn hex(rgb: u32) -> Color32`
(`hex(0xebedef)`). Имя поля = имя токена без `--`, `-` → `_`.

| Токен (`Palette`) | Light | Dark | Источник (light / dark) |
|---|---|---|---|
| `bg` | `#ffffff` | `#2c3035` | `commit_list_bg` = `light_gray` / `dark_gray-lightest` |
| `text` | `#333333` | `#aaaaaa` | `text` |
| `text_strong` | `#1a1a1a` | `#dddddd` | `dark_gray` / `text-heading` |
| `text_muted` | `#5a636d` | `#939eaa` | `medium_gray`, затемнён (light) и осветлён (dark) до 4.5:1 (WCAG AA) на `bg`, `sidebar_bg`, `detail_bg`; держит тест `muted_text_meets_wcag_aa` |
| `header_bg` | `#e6e6e6` | `#1e2124` | `header_bg` |
| `header_fg` | `#333333` | `#dddddd` | `header_fg` |
| `sidebar_bg` | `#f4f4f4` | `#25292d` | `side_bar_container_bg` |
| `sidebar_fg` | `#333333` | `#aaaaaa` | `location_bar_fg` |
| `sidebar_heading_fg` | `#8e8e8e` | `#dddddd` | `location_bar_heading_fg` |
| `row_hover_bg` | `#e8e8e8` | `#32383e` | `location_bar_row_bg-hover` / `commit_row_bg-hover` |
| `row_selected_bg` | `#d0e6f8` | `#435970` | `light_blue` / `dark_blue` |
| `row_selected_fg` | `#1a1a1a` | `#dddddd` | `dark_gray` / `text-heading` |
| `row_disabled_fg` | `#b0b0b0` | `#505962` | `medium_gray-light` / `medium_gray-dark` |
| `border` | `#dcdcdc` | `#17191c` | `light_gray-dark` / `dark_gray` |
| `accent` | `#4a90d9` | `#81b2e4` | `blue` |
| `dirty` | `#e68619` | `#eb9e47` | `orange` |
| `clean` | `#3d9933` | `#5cd65c` | `green` |
| `badge_bg` | `#e4e4e4` | `#505962` | `medium_gray-light` / `medium_gray-dark` |
| `badge_fg` | `#555555` | `#dddddd` | — |
| `button_bg` | `#ffffff` | `#66696c` | `button_bg` |
| `button_fg` | `#333333` | `#dddddd` | `button_fg` = `label_color` |
| `button_hover_bg` | `#e8e8e8` | `#737f8c` | `light_gray-dark` / `medium_gray` |
| `button_primary_bg` | `#4a90d9` | `#4c89c7` | `highlighted_button_dark_bg` |
| `button_primary_fg` | `#ffffff` | `#ffffff` | `dark_gray` |
| `button_danger_bg` | `#c75050` | `#704343` | `darker_red` / `dark_red` |
| `button_danger_fg` | `#ffffff` | `#dddddd` | `hazard_button_fg` |
| `button_disabled_opacity` (`f32`) | `0.4` | `0.4` | `icon_button_opacity-faded` |
| `input_bg` | `#ffffff` | `#17191c` | — / `dark_gray` |
| `error_bg` | `#ffdd80` | `#704343` | `error_hint_bg` |
| `error_fg` | `#000000` | `#dddddd` | `error_hint_fg` / `text-heading` |
| `error_accent` | `#e68619` | `#eb9e47` | `error_hint_accent` / `command_status_label_error_fg` |
| `focus_ring` | `#4a90d9` | `#aaaaaa` | `blue` / `focus_highlight_color` |
| `diff_file_bg` | `#eef1f4` | `#353b42` | diff view (не из встроенной темы; syntax — Mariana / One Light) |
| `diff_file_fg` | `#1a1a1a` | `#dddddd` | diff view (не из встроенной темы; syntax — Mariana / One Light) |
| `diff_hunk_bg` | `#e6eef7` | `#313840` | diff view (не из встроенной темы; syntax — Mariana / One Light) |
| `diff_hunk_fg` | `#5a6b7d` | `#8a9aab` | diff view (не из встроенной темы; syntax — Mariana / One Light) |
| `diff_add_bg` | `#e3f4dd` | `#2d4034` | diff view (не из встроенной темы; syntax — Mariana / One Light) |
| `diff_add_fg` | `#3d9933` | `#5cd65c` | diff view (не из встроенной темы; syntax — Mariana / One Light) |
| `diff_remove_bg` | `#fbe4e4` | `#4a3236` | diff view (не из встроенной темы; syntax — Mariana / One Light) |
| `diff_remove_fg` | `#cc3333` | `#ec5f66` | diff view (не из встроенной темы; syntax — Mariana / One Light) |
| `syntax_keyword` | `#a626a4` | `#c695c6` | diff view (не из встроенной темы; syntax — Mariana / One Light) |
| `syntax_type` | `#c18401` | `#5fb4b4` | diff view (не из встроенной темы; syntax — Mariana / One Light) |
| `syntax_string` | `#50a14f` | `#99c794` | diff view (не из встроенной темы; syntax — Mariana / One Light) |
| `syntax_number` | `#986801` | `#f9ae58` | diff view (не из встроенной темы; syntax — Mariana / One Light) |
| `syntax_comment` | `#9a9ba3` | `#7d8996` | diff view (не из встроенной темы; syntax — Mariana / One Light) |

#### 8.3.1 Отображение токенов на поля `egui::Visuals`

`visuals(choice)` начинается с `Visuals::dark()` или `Visuals::light()` и перезаписывает поля (через построение новой структуры,
без мутации общих данных):

| Поле `egui::Visuals` | Токен |
|---|---|
| `dark_mode` | `true` для Dark, `false` для Light |
| `override_text_color` | `None` (цвет текста идёт из `widgets.*.fg_stroke`) |
| `panel_fill` | `bg` |
| `window_fill` | `bg` |
| `window_stroke.color` | `border` |
| `faint_bg_color` | `row_hover_bg` (чередование строк, `striped`) |
| `extreme_bg_color` | `input_bg` (фон `TextEdit`, `ComboBox`, полос прокрутки) |
| `code_bg_color` | `sidebar_bg` |
| `hyperlink_color` | `accent` |
| `warn_fg_color` | `dirty` |
| `error_fg_color` | `error_accent` |
| `selection.bg_fill` | `row_selected_bg` |
| `selection.stroke.color` | `row_selected_fg` |
| `text_cursor.stroke.color` | `accent` |
| `widgets.noninteractive.bg_fill` / `weak_bg_fill` | `bg` |
| `widgets.noninteractive.bg_stroke.color` | `border` (разделители, рамки панелей) |
| `widgets.noninteractive.fg_stroke.color` | `text` |
| `widgets.inactive.bg_fill` / `weak_bg_fill` | `button_bg` |
| `widgets.inactive.fg_stroke.color` | `button_fg` |
| `widgets.inactive.bg_stroke.color` | `border` |
| `widgets.hovered.bg_fill` / `weak_bg_fill` | `button_hover_bg` |
| `widgets.hovered.fg_stroke.color` | `text_strong` |
| `widgets.hovered.bg_stroke.color` | `focus_ring` (фокус и наведение) |
| `widgets.active.bg_fill` / `weak_bg_fill` | `row_selected_bg` |
| `widgets.active.fg_stroke.color` | `row_selected_fg` |
| `widgets.active.bg_stroke.color` | `accent` |
| `widgets.open.*` | как `widgets.hovered.*` |
| `disabled_alpha` (если поле есть в версии eframe) | `button_disabled_opacity` |

Токены, которых нет в `Visuals`, используются в `app.rs` напрямую из `Palette`: `header_bg/fg` (`Frame` верхней панели),
`sidebar_bg/fg/heading_fg` (`Frame` и заголовки Locations), `row_disabled_fg` (строка `.bare`), `dirty`/`clean` (точка и ✓),
`badge_bg/fg` (бейджи `bare`/`plain`), `button_primary_*` (Create), `button_danger_*` (Remove), `error_*` (нижняя панель),
`text_muted` (статус, HEAD).

### 8.4 Фоновые задачи

Git и smerge не вызываются в потоке UI.

- Действие (скан, branches, open, create, remove) запускается в `std::thread::spawn`, результат возвращается через `std::sync::mpsc`
  как `enum JobResult { Scanned(AppResult<Vec<Repo>>), Branches(..), Opened(..), Created(..), Removed(..) }`.
  Поток после отправки вызывает `ctx.request_repaint()`.
- Одновременно выполняется не больше одной задачи. Пока задача идёт, кнопки действий неактивны, в статусе — `egui::Spinner`.
- После Create и Remove автоматически выполняется Rescan. Снимок заменяется целиком (`Arc<[Repo]>`); выбор сохраняется, если путь
  ещё существует в новом снимке, иначе сбрасывается.
- Первый скан стартует в `WorktreeBarApp::new`.

---

## 9. Тесты (`cargo test`)

### 9.1 Изоляция

- Все репозитории создаются в `tempfile::TempDir`. `~/repos`, `/Applications` и реальные репозитории не читаются и не меняются.
- Переменные окружения процесса не меняются (`std::env::set_var` в тестах запрещён). Каждый тест строит
  `Config { roots: vec![<tmp canonicalized>], smerge: <tmp>/fake-smerge, git_env }` через `common::test_config(&tmp)`, где `git_env`:
  `GIT_CONFIG_GLOBAL=/dev/null`, `GIT_CONFIG_NOSYSTEM=1`, `HOME=<tmp>/home`, `GIT_AUTHOR_NAME/EMAIL`, `GIT_COMMITTER_NAME/EMAIL`.
- Фикстуры тоже вызывают git через `Command` с argv и тем же окружением.
- `fake-smerge` — исполняемый файл в `<tmp>` (права `0o755`), который дописывает свои аргументы построчно в `<tmp>/smerge-calls.txt`.
  Внешний редактор не запускается.

Фикстура `make_bare_layout(tmp, "grp/repo", branches = ["master", "test", "feature/x"], worktrees = ["master", "test"])`:
```
["git", "init", "-b", "master", "<tmp>/.seed"]; запись README; ["git", "-C", "<tmp>/.seed", "add", "README"]; ["git", "-C", "<tmp>/.seed", "commit", "-m", "init"]
["git", "clone", "--bare", "<tmp>/.seed", "<tmp>/grp/repo/.bare"]
["git", "--git-dir=<tmp>/grp/repo/.bare", "branch", "test", "master"]          (и другие ветки)
["git", "--git-dir=<tmp>/grp/repo/.bare", "worktree", "add", "<tmp>/grp/repo/master", "master"]
["git", "--git-dir=<tmp>/grp/repo/.bare", "worktree", "add", "<tmp>/grp/repo/test", "test"]
```
(`commit` здесь — в фикстуре теста над временным seed-репозиторием, не в `src/`.)
Фикстура `make_plain_clone(tmp, "plain")`: `["git", "clone", "<tmp>/.seed", "<tmp>/plain"]`. Каталог `.seed` скрытый, поэтому обход его не берёт.

### 9.2 Обязательные кейсы

`tests/git_ops.rs`:

| Тест | Шаги | Ожидание |
|---|---|---|
| `discover_bare_layout` | фикстура bare; `discover_repos(&config)` | ровно один `Repo`: `Layout::Bare`, `name == "grp/repo"`, `root == canonical(<tmp>/grp/repo)`; worktree `master` и `test` с `layout_correct`, `dirty == Some(false)`, `head` из 7 hex; одна запись `bare == true` с путём `.bare` |
| `discover_does_not_descend_into_layout` | создать `<tmp>/grp/repo/master/nested/.bare` (пустой каталог) | репозиторий найден один раз, `nested` не найден |
| `discover_skips_hidden_and_node_modules` | настоящие bare в `<tmp>/.hidden/r/.bare` и `<tmp>/node_modules/r/.bare`, plain-клон | скрытые и `node_modules` отсутствуют; `grp/repo` и `plain` есть |
| `bare_entry_not_openable` | `open_in_merge(&config, <root>/.bare)`; плюс проверка, что `app` не формирует для записи `bare` действие Open (функция `app::openable(&Worktree) -> bool` возвращает `false`) | `Err(code == BareNotOpenable)`; `smerge-calls.txt` не создан |
| `parent_not_openable` | `open_in_merge(&config, <root>)` | `ParentNotOpenable`; smerge не вызван |
| `open_passes_only_worktree_path` | `open_in_merge(&config, <root>/test)` | в `smerge-calls.txt` ровно одна строка `<canonical root>/test` |
| `create_refuses_branch_in_use` | `create_worktree(&config, root, "test")` | `BranchInUse`, message содержит `<root>/test`; список worktree не изменился |
| `create_worktree_for_slash_branch` | `create_worktree(&config, root, "feature/x")` | каталог `<root>/feature/x` существует, `branch == Some("feature/x")`, `layout_correct` |
| `create_refuses_existing_dest` | `fs::create_dir_all(<root>/feature/x)`, затем create | `DestExists`; список worktree не изменился |
| `create_refuses_nested_dest` | worktree для ветки `feature`, ветка `feature/y` существует | `DestNested` |
| `invalid_branch_names` | `..`, `../x`, `a/../b`, `/abs`, `.bare`, `x/.bare`, `-f`, `a//b`, `a/`, `.hidden`, `""` | каждый даёт `InvalidBranch`; `layout_path_for` тоже `Err` |
| `remove_refuses_dirty` | untracked `<root>/test/new.txt`; `remove_worktree(&config, <root>/test)` | `DirtyWorktree`; каталог и файл на месте |
| `remove_clean_keeps_bare` | `remove_worktree` на чистом `<root>/test` | `<root>/test` нет; `<root>/.bare` есть; `refs/heads/test` остался |
| `remove_bare_refused` | `remove_worktree(&config, <root>/.bare)` | `BareNotOpenable`; `.bare` на месте |
| `path_outside_roots_refused` | `open_in_merge(&config, "/etc")`, путь в другом `TempDir` | `OutsideRoots` |
| `plain_clone_marked_plain` | `open_path(&config, <tmp>/plain)`; затем `create_worktree` для его root | `Layout::Plain`, один worktree; create → `PlainLayout` |
| `config_from_values_defaults` | `Config::from_values(<tmp>/home, None, None)` при созданном `<tmp>/home/repos` | roots = `<tmp>/home/repos`; smerge = `DEFAULT_SMERGE` |
| `roots_are_temp_only` | `common::test_config` | каждый root лежит под `tmp`; ни один не равен и не лежит под настоящим `~/repos` |
| `source_has_no_forbidden_git` | `include_str!` для `src/git.rs`, `src/discover.rs`, `src/model.rs` (строковые литералы, точное совпадение) и для всех `src/*.rs` (вызовы) | в первых трёх файлах нет литералов `"checkout"`, `"switch"`, `"reset"`, `"clean"`, `"merge"`, `"rebase"`, `"prune"`, `"--force"`, `"-f"`, `"-D"`, `"sh"`, `"-c"`; во всех `src/*.rs` нет `remove_dir_all`, `remove_file`, `git2` (тексты UI в `app.rs`, например «clean», под литеральную проверку не попадают). `commit`, `push`, `fetch`, `pull` разрешены как argv из §7.0 |

Юнит-тесты (`#[cfg(test)]`):

| Модуль | Тест | Ожидание |
|---|---|---|
| `git.rs` | `parse_porcelain_bare_and_branches` | разбор фиксированной строки porcelain: запись `bare`, ветки `master`, `feature/x`, `detached`, `locked reason`, `prunable` |
| `git.rs` | `run_with_timeout_kills` | `run_with_timeout(Command::new("sleep").arg("5"), 100 мс)` → `Err(code == GitTimeout)` за < 2 с |
| `theme.rs` | `both_themes_construct` | `visuals(Dark).dark_mode == true`, `visuals(Light).dark_mode == false`; `panel_fill == palette(..).bg`, `selection.bg_fill == row_selected_bg`, `extreme_bg_color == input_bg` для обеих тем |
| `theme.rs` | `palettes_match_spec_hex` | все поля `LIGHT`/`DARK` равны hex из §8.3 (список значений захардкожен в тесте) |
| `theme.rs` | `theme_file_roundtrip` | `save_theme_choice(tmp, Light)` → файл содержит `light`; `load_theme_choice` → `Some(Light)`; мусор в файле → `None`; нет файла → `None` |
| `theme.rs` | `config_dir_path` | `config_dir("/h") == "/h/Library/Application Support/hydra-merge"` |

Покрытие: 80% строк `src/git.rs`, `src/discover.rs`, `src/theme.rs`, `src/model.rs` (код отрисовки в `app.rs` и `main.rs` из порога
исключён). Если локально установлен `cargo-llvm-cov`: `cargo llvm-cov --fail-under-lines 80 --ignore-filename-regex 'src/(app|main)\.rs'`.
Если его нет, проверяется только прохождение `cargo test`, и в отчёте это пишется явно. В зависимости проекта он не добавляется.

---

## 10. README.md (содержание после переписывания)

1. Что это и зачем (2–3 предложения, ссылка на `docs/spec.md`).
2. Сборка и запуск: `cargo run --release` или `target/release/hydra-merge`. Нативное окно, сети нет, сервера нет.
3. Переменные: `HYDRA_MERGE_ROOTS`, `HYDRA_MERGE_SMERGE`.
4. Тема: переключатель в окне, файл `~/Library/Application Support/hydra-merge/theme.txt`.
5. Что приложение делает (open, create, remove) и чего не делает никогда (checkout/reset/clean/push/commit, `--force`, удаление `.bare`,
   конвертация клонов).
6. Тесты: `cargo test`. Тесты работают только с временными репозиториями.

Упоминаний Python, `http.server`, порта, `127.0.0.1`, `static/` в README не остаётся.

---

## 11. Критерии приёмки

- Python-файлы из §1.2 удалены; `ls` в корне показывает `Cargo.toml`, `Cargo.lock`, `src/`, `tests/`, `docs/`, `README.md`.
- `cargo build --release` и `cargo test` проходят; `cargo clippy -- -D warnings` без предупреждений.
- Тесты не трогают `~/repos`, `/Applications` и переменные окружения процесса.
- `target/release/hydra-merge` открывает окно с заголовком `HydraMerge`; `lsof -p <pid> -i` не показывает сетевых сокетов.
- На реальном локальном репозитории с раскладкой bare + ветки (ручная проверка пользователем) в Locations видны `master`, `test` и
  `.bare` (bare, неактивна). Open открывает в редакторе папку ветки.
- Обе темы полностью применяются; выбор переживает перезапуск через `theme.txt`; без файла тема следует системе, а если система
  не сообщила внешний вид — тёмная.
- Любая ошибка из §5.2 видна в нижней панели окна.

---

## 12. Изменения 2.1 (2026-10-09)

Раздел описывает поведение, добавленное после версии 2.0. При расхождении с разделами выше действует этот раздел.

- **Редактор.** Open открывает worktree в Sublime Text (`subl`). Переменная `HYDRA_MERGE_SMERGE` и имена `open_in_merge`/`MergeNotFound` сохранены для совместимости.
- **Записи в worktree.** Разрешены stage/unstage/discard, commit, fetch, pull и push (§7.0), а также apply/drop stash.
- **Удаление удалённой ветки.** Команда: `git --git-dir=<root>/.bare push --delete <remote> refs/heads/<branch>`, без `--force`. Затем `update-ref -d refs/remotes/<remote>/<branch>`, если ссылка осталась. Remote должен быть в `git remote`, ссылка `refs/remotes/<remote>/<branch>` должна существовать. Перед действием окно показывает подтверждение.
- **Контекстное меню веток (правый клик).**
  - На удалённой ветке: Create Local Branch (`git branch --track <branch> <remote>/<branch>`, с тем же запасным путём без `--track`), Create Worktree, Delete (с подтверждением), Copy name.
  - На локальной ветке: Open in Sublime Text и Show Worktree, если ветка занята; Create Worktree, если свободна; Copy name.
  - Checkout, Merge и Rebase в это меню не добавляются (§ «What it never does»).
- **Без обхода корней.** Проверка пути перед действием (§6) больше не сканирует discovery roots:
  1. `git -C <path> rev-parse --git-common-dir` находит репозиторий.
  2. Если это `<root>/.bare`, используется bare-раскладка.
  3. Если это `<toplevel>/.git` обычного репозитория под корнями сканирования или из «Open path…», используется plain.
  4. Читается только список worktree этого репозитория, без `git status`.
  5. Путь, содержащий сегмент `.bare`, возвращает `BareNotOpenable`.
  - После любого действия перечитывается только затронутый репозиторий. Полный скан выполняется только по Rescan и Auto.
- **Создание из удалённой ветки.** Используется `worktree add --track -b`. Если git не может настроить tracking (bare-клон без `remote.<name>.fetch`), worktree создаётся без upstream, причина пишется в stderr.
- **Rename в status.** В `status -z` после записи `R`/`C` идёт старый путь. Строка показывается как `old → new`, Unstage передаёт оба пути.
- **Окно.**
  - Ошибка в нижней панели не останавливает загрузку веток, истории и diff. Неудачная загрузка запоминается и не повторяется до смены выбора.
  - Колонка истории меняет ширину перетаскиванием разделителя.
  - Секции Locations сворачиваются. Ветки со `/` сгруппированы в сворачиваемые папки, при поиске папки раскрыты.
  - У вкладок репозиториев есть полоса прокрутки.
- **Diff.** Номера строк, цвет добавленных и удалённых строк, подсветка синтаксиса (токены `diff_*` и `syntax_*` в §8.3). Патч длиннее 200 000 байт обрезается по границе строки.

### 12.1 Интерфейс (2026-10-09)

- **Раскладка.** Вкладки репозиториев (× закрывает вкладку, ▾ — список всех, + — Open path), тулбар, Locations, список коммитов, правая панель. Нижняя строка статуса и ошибок осталась.
- **История.** `git log --branches --remotes --tags HEAD --date-order -n 200 --decorate=full` с `%P` и `%D`. Граф строит `src/graph.rs`: дорожки по родителям, первый родитель остаётся на дорожке коммита.
- **Рабочая копия.** Первая строка списка («N changes»). Её панель Summary показывает сообщение коммита, кнопку Commit, unstaged/staged файлы и `git diff HEAD`.
- **Выбор в Locations.** Ветка, тег или stash больше не сбрасывают выбранный worktree: их детали открываются в правой панели рядом со списком коммитов.
- **Палитра.** Добавлены токены `toolbar_bg`, `badge_head_*`, `graph[6]`. Светлая тема отделена от тёмной.
- **Зависимости.** Добавлена dev-зависимость `egui_kittest` (`eframe`, `wgpu`, `snapshot`) для офскрин-рендера окна в тесте `tests/ui_shots.rs` (`#[ignore]`). В приложение она не линкуется, runtime-зависимостей по-прежнему только `eframe`.

### 12.2 Сверка раскладки окна (2026-10-09)

- **Палитра.** Светлая палитра снята с кадров записи: вкладки `#adb6bd`, тулбар и шапки панелей `#c9cfd3`, Locations `#dddfe1`, список коммитов `#ebedef`, правая панель `detail_bg` `#fbfbfb`, полосы секций `section_bg` `#d0d6d8`, выбор `#d2e1ed`.
- **Summary.** Поле сообщения без рамки (моноширинное). Строка автора из `git config user.name/user.email`. Справа широкая кнопка Commit. Сворачиваемые полосы Staged / Unstaged / Untracked Files со счётчиком и собственным diff (`git diff --cached` и `git diff`).
- **Строка рабочей копии.** «No changes», «N unstaged files», «N staged files» или «N staged, M unstaged files»; untracked считаются unstaged.
- **Остальное.**
  - Заголовок окна показывает путь открытого worktree (`~/…`).
  - Locations: заголовок по центру с иконкой поиска (фильтр раскрывается по клику), жирные заголовки секций, пустые секции свёрнуты.
  - Меню `⋯` стоит рядом с полем ветки по центру тулбара.
  - Загрузка показана тремя пульсирующими квадратами.

### 12.3 Палитра команд и навигация (2026-10-09)

- **Палитра (`src/app/palette.rs`).**
  - Открывается кликом по полю `>_` в центре тулбара или сочетанием ⌘⇧P.
  - Нечёткий поиск по подпоследовательности, с бонусом за начало слова и подряд идущие буквы. Недоступные команды скрыты.
  - Пустой запрос показывает последние 10 команд (в памяти) или «No previous command history».
  - Команды выполняются как обычные `Action`: деструктивные проходят через то же подтверждение, во время фоновой задачи действие ждёт в очереди.
- **Навигация (`src/app/nav.rs`).**
  - «Место» — это вкладка, worktree, открытый коммит и выбор в Locations.
  - После каждого действия пользователя, которое меняет место, предыдущее место уходит в стек «назад» (до 100 мест), стек «вперёд» сбрасывается.
  - ◀ ▶ в тулбаре и ⌘[ / ⌘] переходят по истории. Изменения, пришедшие из фоновых задач, в историю не записываются.

### 12.4 Многопоточность и фоновый «индекс» (2026-10-09)

- **Пул потоков (`src/pool.rs`).** `parallel_map` на `std::thread::scope`, число потоков — CPU, но не больше 8. Новых зависимостей нет.
- **Discovery.** Обход каталогов идёт без git. Проверки `.bare` и `worktree list` выполняются параллельно, результат тот же, что у последовательного скана.
- **Двухфазный скан.** Скан в окне идёт со `Status::Skip`, список появляется сразу. `git status` всех worktree читает отдельная «полоса статусов» (`read_statuses`, параллельно) и не занимает слот действий. Если репозиторий за это время перечитан, его статусы из полосы игнорируются.
- **Полоса загрузок.** Ветки, история и патч активной вкладки грузятся отдельно от действий (fetch/commit/…), по одной загрузке за раз.
- **Кэш.**
  - Ветки хранятся по репозиторию, история — по worktree, патчи — по хешу (до 64 записей в каждом кэше).
  - Возврат на вкладку показывает кэш сразу и перечитывает его в фоне. Патчи коммитов неизменны и не перечитываются.
  - Каждый скан, перезагрузка репозитория и запись увеличивают номер поколения. Ответы загрузок старого поколения отбрасываются.
- **Замер на локальных корнях** (36 репозиториев, 61 worktree): список готов за ~0,3 с, статусы приходят в фоне через ~0,5–0,9 с.
- **Сравнение по `find` до глубины 5.** На более широком наборе (159 репозиториев, 251 worktree) прежний последовательный `git status` занимал 14,2 с, параллельный — 2,3 с.

### 12.5 Десять улучшений интерфейса (2026-10-09)

| # | Улучшение | Что сделано |
|---|---|---|
| 1 | #2201, #2196, #1157 | Внешние изменения отслеживаются отпечатком mtime (`src/watch.rs`: HEAD, packed-refs, index, FETCH_HEAD, `refs/`, `worktrees/`): активный репозиторий проверяется каждые 3 с, все — каждые 30 с. Изменившийся репозиторий перечитывается. При фокусе окна и каждые 5 с перечитывается открытый worktree (в фоне) |
| 2 | #25 | Auto-fetch: `git --git-dir=<.bare> fetch --all` открытого репозитория раз в 5 минут, на отдельном потоке. Включается в ⋯ / палитре, состояние хранится в сессии |
| 3 | #42 | После push ссылка из вывода сервера (merge request / pull request) показывается кнопкой. В `/usr/bin/open` передаются только `https://` ссылки без пробелов и управляющих символов |
| 4 | #255 | Compare: `git diff base...target` между веткой текущего worktree и выбранной веткой. Ссылки проверяются (`ref_name_ok` + `rev-parse --verify`) |
| 5 | #396, #789, #1882 | Сессия в `session.txt`: окно, вкладки, закрытые вкладки, выбор, ширины, Locations, auto-fetch, недавние команды. Запись через временный файл и rename. **Разрешено пользователем** |
| 6 | #186 | Open in Terminal (`open -a <app> <path>`, `HYDRA_MERGE_TERMINAL`) и Show in Finder (`open -R <path>`: папка с именем `*.app` выделяется, а не запускается), только для проверенного пути worktree |
| 7 | #1106, #147, #1099 | PATH дочерних процессов дополняется `/opt/homebrew/{bin,sbin}`, `/usr/local/{bin,sbin}`, `/etc/paths`, `/etc/paths.d/*` |
| 8 | #214, #44, #1039 | Ошибки входа (`could not read Username`, `Permission denied (publickey)` …) дают `ErrorCode::AuthRequired` с подсказкой и кнопкой «Open in Terminal» |
| 9 | #1626, #352 | Фильтр списка коммитов (⌘F) по теме, автору, хешу и веткам. «Search all history»: `git log --regexp-ignore-case --fixed-strings --grep=<text>` (одним элементом argv) |
| 10 | #381, #143 | Выбор нескольких файлов (клик, ⌘, Shift) и Stage / Unstage / Discard Selected. «Stage Folder» в меню файла (`git add -- <dir>/`) |
| 11 | — | Доверие к репозиторию: если собственный конфиг или хуки репозитория запускают программы (`filter.*`, `merge.*.driver`, `core.sshCommand`, `core.gitProxy`, `core.hooksPath`, `credential.*helper`, исполняемый хук в `hooks/` и др.), он не читается и не меняется, пока пользователь не нажмёт «Trust this repository». Решение хранится в `session.txt` вместе с отпечатком этих настроек и сбрасывается, если они изменились |

### 12.6 Окно «Open Repository» (2026-10-10, по просьбе пользователя)

- «Open path…» больше не строка в тулбаре, а модальное окно: поле пути, «Choose…», Cancel / Open. Открывается через `+` у вкладок, ⚙ → Open Repository by Path… и палитру. Enter открывает репозиторий, Esc закрывает окно. Ошибка (`OutsideRoots`, `NotARepo` …) показывается в самом окне.
- **Изменение §1.1 и §5.6.** Для «Choose…» разрешён системный выбор папки. Он работает через `/usr/bin/osascript` с фиксированным скриптом `choose folder` (`src/git/editor.rs`, `choose_folder`), а начальная папка передаётся отдельным аргументом. Shell не используется, новых зависимостей (`rfd`) нет, тайм-аут 10 минут, отмена (-128) — не ошибка. Выбранный путь проходит обычные проверки `open_path` (под discovery roots и т. д.).

### 12.7 Скины (2026-10-11)

- **Встроенные скины** (`src/skin/seeds.rs`). Merge Light и Merge Dark — ручные палитры §8.3. Mariana, One Dark, Monokai, Nord, Solarized Dark/Light и GitHub Light задаются примерно двадцатью базовыми цветами (`Seed`), остальные токены выводятся смешиванием (`mix`). Тест проверяет читаемость каждого скина: контраст текста и фона, активной вкладки и полосы, добавленных и удалённых строк, текста основной кнопки.
- **Свои скины.** Файлы `<config>/skins/*.skin`, формат `key = value`: `base`, `dark`, любой цветовой токен `#rrggbb`, `graph` (шесть цветов), `button_disabled_opacity`. Строки, которые нельзя применить, дают предупреждения с номером строки; остальное применяется. Имя, совпадающее со встроенным, получает суффикс «(custom)».
- **Выбор.** ⚙ → Skin или «Skin: …» в палитре; выбор хранится в `<config>/skin.txt`. «Auto» — прежнее поведение (Merge Light/Dark, система и переключатель темы). Выбранный скин задаёт режим egui (dark/light) и его visuals (`theme::visuals_for`). Переключатель темы возвращает к Auto.
- **Создание и правка.** «Create Skin from Current…» пишет текущую палитру в новый файл (`<имя> copy.skin`, при совпадении `… 2`) и открывает его в Sublime Text (`subl <файл>`). Папка скинов проверяется каждые 1,5 с, сохранённый файл применяется сразу. «Open Skins Folder» открывает папку через `open <dir>`.
- **Новые записи в config dir:** `skin.txt` и `skins/*.skin` (дополняет §8.2 и §12.5).

### 12.8 Open With (2026-10-11)

- **Выбор редактора.** Open, «Open in …», открытие подмодуля и новых `.skin` идут через `git::open_with` и выбранный `Editor`:
  - `SublimeCli` — прежний `subl` из `HYDRA_MERGE_SMERGE`;
  - `App { name, path }` — `open -a <путь к .app> <папка>`, принимается только существующий абсолютный `*.app`.
- **Список.** Sublime Text, плюс найденные в `/Applications` и `~/Applications` Visual Studio Code, Cursor, Zed, Windsurf, IntelliJ IDEA (CE), GoLand, PyCharm (CE), RustRover, WebStorm, Nova и BBEdit. «Other App…» открывает системный выбор приложения (`osascript`, `choose application`, отмена — не ошибка).
- **Где выбирать.** ⚙ → Open With и «Open With: …» в палитре. Выбор хранится в `session.txt` (`editor_app`); пусто — Sublime Text.
