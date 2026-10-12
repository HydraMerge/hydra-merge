//! Language table: comment and string syntax, keywords, and detection by file name.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Plain,
    Rust,
    CLike,
    Go,
    Jvm,
    Script,
    Swift,
    CSharp,
    Python,
    Ruby,
    Shell,
    Yaml,
    Toml,
    Hcl,
    Json,
    Sql,
    Lua,
    Php,
    Markup,
    Css,
    Docker,
}

pub(super) struct Syntax {
    pub(super) line_comments: &'static [&'static str],
    pub(super) block_comment: Option<(&'static str, &'static str)>,
    pub(super) quotes: &'static [char],
    /// Delimiters that may span lines; the same text closes them.
    pub(super) multi_strings: &'static [&'static str],
    /// Space-separated words.
    pub(super) keywords: &'static str,
    pub(super) literals: &'static str,
    /// First word on a line followed by this character is a key (YAML `name:`, TOML `name =`).
    pub(super) key_sep: Option<char>,
    /// Extra characters allowed inside identifiers (YAML keys like `ansible-lint`).
    pub(super) ident_extra: &'static str,
    /// `<tag` and `</tag` are keywords.
    pub(super) tags: bool,
    pub(super) case_insensitive: bool,
    pub(super) types: bool,
}

const NONE: Syntax = Syntax {
    line_comments: &[],
    block_comment: None,
    quotes: &[],
    multi_strings: &[],
    keywords: "",
    literals: "",
    key_sep: None,
    ident_extra: "",
    tags: false,
    case_insensitive: false,
    types: false,
};

const C_COMMENTS: Option<(&str, &str)> = Some(("/*", "*/"));
const BOOLS: &str = "true false null";

const RUST: Syntax = Syntax {
    line_comments: &["//"],
    block_comment: C_COMMENTS,
    quotes: &['"'],
    keywords: "as async await break const continue crate dyn else enum extern fn for if impl in let loop match mod move mut pub ref return self Self static struct super trait type unsafe use where while",
    literals: "true false None Some Ok Err",
    types: true,
    ..NONE
};

const CLIKE: Syntax = Syntax {
    line_comments: &["//"],
    block_comment: C_COMMENTS,
    quotes: &['"', '\''],
    keywords: "auto break case class const constexpr continue default delete do else enum extern for goto if inline namespace new operator private protected public return sizeof static struct switch template this typedef typename union using virtual void volatile while #include #define #ifdef #ifndef #endif int char float double long short unsigned signed bool",
    literals: "true false NULL nullptr",
    types: true,
    ..NONE
};

const GO: Syntax = Syntax {
    line_comments: &["//"],
    block_comment: C_COMMENTS,
    quotes: &['"', '\''],
    multi_strings: &["`"],
    keywords: "break case chan const continue default defer else fallthrough for func go goto if import interface map package range return select struct switch type var",
    literals: "true false nil iota",
    types: true,
    ..NONE
};

const JVM: Syntax = Syntax {
    line_comments: &["//"],
    block_comment: C_COMMENTS,
    quotes: &['"', '\''],
    multi_strings: &["\"\"\""],
    keywords: "abstract break case catch class companion continue data def default do else enum extends final finally for fun if implements import in interface is new object override package private protected public return sealed static super suspend switch this throw throws try val var void when while",
    literals: "true false null",
    types: true,
    ..NONE
};

const SCRIPT: Syntax = Syntax {
    line_comments: &["//"],
    block_comment: C_COMMENTS,
    quotes: &['"', '\''],
    multi_strings: &["`"],
    keywords: "as async await break case catch class const continue default delete do else enum export extends finally for from function if implements import in instanceof interface let new of private public readonly return static super switch this throw try type typeof var void while yield",
    literals: "true false null undefined NaN",
    ident_extra: "$",
    types: true,
    ..NONE
};

const SWIFT: Syntax = Syntax {
    line_comments: &["//"],
    block_comment: C_COMMENTS,
    quotes: &['"'],
    multi_strings: &["\"\"\""],
    keywords: "as break case class continue default defer do else enum extension fileprivate for func guard if import in init internal let private protocol public return self static struct switch throw throws try var where while",
    literals: "true false nil",
    types: true,
    ..NONE
};

const CSHARP: Syntax = Syntax {
    line_comments: &["//"],
    block_comment: C_COMMENTS,
    quotes: &['"', '\''],
    keywords: "abstract async await base break case catch class const continue default else enum finally for foreach if in interface internal namespace new out override private protected public readonly record ref return sealed static struct switch this throw try using var virtual void while",
    literals: "true false null",
    types: true,
    ..NONE
};

const PYTHON: Syntax = Syntax {
    line_comments: &["#"],
    quotes: &['"', '\''],
    multi_strings: &["\"\"\"", "'''"],
    keywords: "and as assert async await break class continue def del elif else except finally for from global if import in is lambda nonlocal not or pass raise return try while with yield self",
    literals: "True False None",
    types: true,
    ..NONE
};

const RUBY: Syntax = Syntax {
    line_comments: &["#"],
    quotes: &['"', '\''],
    keywords: "alias and begin break case class def do else elsif end ensure for if in module next not or redo rescue retry return self super then unless until when while yield",
    literals: "true false nil",
    types: true,
    ..NONE
};

const SHELL: Syntax = Syntax {
    line_comments: &["#"],
    quotes: &['"', '\''],
    keywords: "case do done elif else esac export fi for function if in local readonly return set then until while echo exit",
    literals: "true false",
    ..NONE
};

const YAML: Syntax = Syntax {
    line_comments: &["#"],
    quotes: &['"', '\''],
    literals: "true false yes no null True False Yes No ~",
    key_sep: Some(':'),
    ident_extra: "-.",
    ..NONE
};

const TOML: Syntax = Syntax {
    line_comments: &["#", ";"],
    quotes: &['"', '\''],
    multi_strings: &["\"\"\"", "'''"],
    literals: BOOLS,
    key_sep: Some('='),
    ident_extra: "-.",
    ..NONE
};

const HCL: Syntax = Syntax {
    line_comments: &["#", "//"],
    block_comment: C_COMMENTS,
    quotes: &['"'],
    keywords: "data dynamic for for_each if in locals module output provider resource terraform variable count depends_on",
    literals: BOOLS,
    key_sep: Some('='),
    ident_extra: "-",
    ..NONE
};

const JSON: Syntax = Syntax {
    quotes: &['"'],
    literals: BOOLS,
    key_sep: Some(':'),
    ..NONE
};

const SQL: Syntax = Syntax {
    line_comments: &["--"],
    block_comment: C_COMMENTS,
    quotes: &['\'', '"'],
    keywords: "add alter and as asc begin by case column commit create default delete desc distinct drop else end exists foreign from function grant group having if in index inner insert into is join key left like limit not on or order outer primary references returns right select set table then union unique update values view when where with",
    literals: "true false null",
    case_insensitive: true,
    ..NONE
};

const LUA: Syntax = Syntax {
    line_comments: &["--"],
    quotes: &['"', '\''],
    keywords: "and break do else elseif end for function goto if in local not or repeat return then until while",
    literals: "true false nil",
    ..NONE
};

const PHP: Syntax = Syntax {
    line_comments: &["//", "#"],
    block_comment: C_COMMENTS,
    quotes: &['"', '\''],
    keywords: "abstract array as break case catch class const continue default echo else elseif extends final fn for foreach function if implements interface namespace new private protected public return static switch throw trait try use while",
    literals: "true false null",
    ident_extra: "$",
    types: true,
    ..NONE
};

const MARKUP: Syntax = Syntax {
    block_comment: Some(("<!--", "-->")),
    quotes: &['"', '\''],
    tags: true,
    ..NONE
};

const CSS: Syntax = Syntax {
    block_comment: C_COMMENTS,
    quotes: &['"', '\''],
    keywords: "@media @import @keyframes !important",
    key_sep: Some(':'),
    ident_extra: "-",
    ..NONE
};

const DOCKER: Syntax = Syntax {
    line_comments: &["#"],
    quotes: &['"', '\''],
    keywords: "FROM RUN CMD LABEL EXPOSE ENV ADD COPY ENTRYPOINT VOLUME USER WORKDIR ARG ONBUILD STOPSIGNAL HEALTHCHECK SHELL AS",
    ..NONE
};

impl Lang {
    pub(super) fn syntax(self) -> &'static Syntax {
        match self {
            Lang::Plain => &NONE,
            Lang::Rust => &RUST,
            Lang::CLike => &CLIKE,
            Lang::Go => &GO,
            Lang::Jvm => &JVM,
            Lang::Script => &SCRIPT,
            Lang::Swift => &SWIFT,
            Lang::CSharp => &CSHARP,
            Lang::Python => &PYTHON,
            Lang::Ruby => &RUBY,
            Lang::Shell => &SHELL,
            Lang::Yaml => &YAML,
            Lang::Toml => &TOML,
            Lang::Hcl => &HCL,
            Lang::Json => &JSON,
            Lang::Sql => &SQL,
            Lang::Lua => &LUA,
            Lang::Php => &PHP,
            Lang::Markup => &MARKUP,
            Lang::Css => &CSS,
            Lang::Docker => &DOCKER,
        }
    }
}

/// Language from a file path: well-known file names first, then the extension.
pub fn lang_for_path(path: &str) -> Lang {
    let name = path.rsplit('/').next().unwrap_or(path);
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        "dockerfile" | "containerfile" => return Lang::Docker,
        "makefile" | "gnumakefile" | ".bashrc" | ".zshrc" | ".profile" => return Lang::Shell,
        "jenkinsfile" => return Lang::Jvm,
        "cargo.lock" | "pipfile" => return Lang::Toml,
        _ => {}
    }
    if lower.starts_with("dockerfile.") {
        return Lang::Docker;
    }
    let ext = lower.rsplit_once('.').map_or("", |(_, ext)| ext);
    lang_for_extension(ext)
}

fn lang_for_extension(ext: &str) -> Lang {
    match ext {
        "rs" => Lang::Rust,
        "c" | "h" | "cc" | "cpp" | "cxx" | "hpp" | "hh" | "m" | "mm" | "ino" | "proto" => {
            Lang::CLike
        }
        "go" => Lang::Go,
        "java" | "kt" | "kts" | "scala" | "groovy" | "gradle" => Lang::Jvm,
        "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" | "mts" | "dart" => Lang::Script,
        "swift" => Lang::Swift,
        "cs" => Lang::CSharp,
        "py" | "pyi" => Lang::Python,
        "rb" | "rake" | "gemspec" => Lang::Ruby,
        "sh" | "bash" | "zsh" | "fish" | "ksh" | "mk" | "env" => Lang::Shell,
        "yml" | "yaml" => Lang::Yaml,
        "toml" | "ini" | "cfg" | "conf" | "properties" | "editorconfig" => Lang::Toml,
        "tf" | "tfvars" | "hcl" | "nomad" => Lang::Hcl,
        "json" | "jsonc" | "json5" | "ipynb" => Lang::Json,
        "sql" | "psql" => Lang::Sql,
        "lua" => Lang::Lua,
        "php" => Lang::Php,
        "html" | "htm" | "xml" | "svg" | "vue" | "plist" | "xaml" | "csproj" => Lang::Markup,
        "css" | "scss" | "sass" | "less" => Lang::Css,
        _ => Lang::Plain,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_language_from_path() {
        assert_eq!(lang_for_path("src/app.rs"), Lang::Rust);
        assert_eq!(lang_for_path("roles/x/tasks/main.yml"), Lang::Yaml);
        assert_eq!(lang_for_path("infra/main.tf"), Lang::Hcl);
        assert_eq!(lang_for_path("docker/Dockerfile"), Lang::Docker);
        assert_eq!(lang_for_path("Dockerfile.prod"), Lang::Docker);
        assert_eq!(lang_for_path("Makefile"), Lang::Shell);
        assert_eq!(lang_for_path("web/App.TSX"), Lang::Script);
        assert_eq!(lang_for_path("README"), Lang::Plain);
        assert_eq!(lang_for_path("notes.md"), Lang::Plain);
    }
}
