//! `fab manual`: writes the reference pages of fabrial's docs site from the
//! code, so they cannot drift from it. It walks the clap command tree for
//! the command pages, reads the key tables beside each TOML file's parser
//! for the file pages, and reads the frontmatter of each installed skill.
//!
//! The commands are grouped by who runs them, one page per group, since a
//! reader comes looking for "what do I run to set up", "what does an agent
//! run" or "what does a workflow run", and a page per command would leave
//! most pages a few lines long.

use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

use clap::{Arg, Args, Command};
use fab_core::gates::{self, Gates};
use fab_core::install;
use fab_core::keys::Key;
use fab_core::settings;

use crate::run::Result;
use crate::sequence;

/// The command pages, each with the top-level commands it holds, in order.
const COMMAND_PAGES: &[CommandPage] = &[
    CommandPage {
        file: "setup-commands.md",
        title: "Setup commands",
        label: "Setup",
        description: "Install fabrial into a repository, refresh it, and report where it has drifted.",
        intro: "These commands install fabrial's workflows, gate, skills and agents into a repository, \
                refresh them from a newer build, and report where an installed copy differs from this \
                build. Run them at the repository root, or name it with `--dir`.",
        commands: &["init", "update", "check"],
    },
    CommandPage {
        file: "agent-commands.md",
        title: "Agent commands",
        label: "Agents",
        description: "The commands agents and skills run on a task: claims, gates, review depth, the review \
                      sequence and cleanup.",
        intro: "Agents, and the skills they follow, run these from a task's worktree or the primary \
                checkout. A person can run them too: `fab status` and `fab cleanup` are the ones you are \
                most likely to want by hand.",
        commands: &["claim", "status", "gates", "review-depth", "sequence", "cleanup"],
    },
    CommandPage {
        file: "workflow-commands.md",
        title: "Workflow commands",
        label: "Workflows",
        description: "The commands fabrial's GitHub workflows run: the review gate, Branch PR, the bot's \
                      comments and reviews, and fabrial's release steps.",
        intro: "fabrial's GitHub workflows run these. When one fails in GitHub Actions it prints the error \
                as an annotation. Most read their input from the environment a workflow sets, so running \
                them by hand is for testing a workflow.",
        commands: &["gate", "branch-pr", "pr-status", "pr-review", "pr-answer", "release"],
    },
    CommandPage {
        file: "pi-commands.md",
        title: "Pi harness commands",
        label: "Pi harness",
        description: "The commands that run Pi workers: fab run, fab dispatch, fab worker and fab events.",
        intro: "These drive the Pi harness, which runs each step of a task in a fresh, sandboxed Pi worker \
                in a tmux session on the host. They exist on Unix only.",
        commands: &["run", "dispatch", "worker", "events"],
    },
];
/// The order of the first page after the command pages.
const FILE_PAGES_ORDER: usize = COMMAND_PAGES.len() + 1;

/// What `fab manual` writes.
#[derive(Args)]
pub struct Options {
    /// The directory to write the pages into, such as
    /// packages/website/src/content/docs/reference. Files it does not
    /// write are left alone.
    #[arg(long)]
    out: PathBuf,
}

/// One generated page.
pub struct Page {
    /// The file name, relative to the output directory.
    pub file: &'static str,
    /// The page's Markdown, frontmatter first.
    pub text: String,
}

/// A page of commands.
struct CommandPage {
    file: &'static str,
    title: &'static str,
    label: &'static str,
    description: &'static str,
    intro: &'static str,
    commands: &'static [&'static str],
}

/// Writes every page into the directory `options` names, creating it.
pub fn run(options: Options, cli: Command) -> Result<()> {
    let pages = pages(cli)?;
    fs::create_dir_all(&options.out).map_err(|e| format!("{}: {e}", options.out.display()))?;
    for page in &pages {
        let path = options.out.join(page.file);
        fs::write(&path, &page.text).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("wrote {}", path.display());
    }
    Ok(())
}

/// Every reference page, from the command tree `cli` and this build's
/// files. Fails when a visible command is on no page or on two, or a page
/// holds text the docs site would read as markup.
pub fn pages(mut cli: Command) -> Result<Vec<Page>> {
    cli.build();
    let visible: Vec<&str> = cli
        .get_subcommands()
        .filter(|c| listed(c))
        .map(Command::get_name)
        .collect();
    let placed: Vec<&str> = COMMAND_PAGES.iter().flat_map(|p| p.commands.iter().copied()).collect();
    for name in &visible {
        match placed.iter().filter(|p| *p == name).count() {
            1 => {}
            0 => {
                return Err(format!(
                    "fab manual: `fab {name}` is on no page; add it to COMMAND_PAGES."
                ));
            }
            _ => return Err(format!("fab manual: `fab {name}` is on more than one page.")),
        }
    }
    let mut pages = Vec::new();
    for (n, page) in COMMAND_PAGES.iter().enumerate() {
        let mut body = wrap(page.intro);
        for name in page.commands {
            // Commands built only on Unix are missing elsewhere.
            if let Some(command) = cli.get_subcommands().find(|c| c.get_name() == *name && listed(c)) {
                command_section(&mut body, command, 2);
            }
        }
        let front = frontmatter(page.title, Some(page.label), page.description, "Commands", n + 1);
        pages.push(Page { file: page.file, text: front + &body });
    }
    pages.push(settings_page());
    pages.push(gates_page());
    pages.push(sequence_page()?);
    pages.push(skills_page()?);
    let errors: Vec<String> = pages.iter().filter_map(|p| check_markup(p).err()).collect();
    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }
    Ok(pages)
}

/// The page for `.github/fabrial.toml`.
fn settings_page() -> Page {
    let mut body = wrap(
        "`.github/fabrial.toml` holds a repository's fabrial settings. The workflows, the review gate and \
         the skills read it. `fab init` writes it, and `fab update` changes a value its flags name. The \
         workflows read each value with `sed`, so a value is one line, `key = \"value\"`, with no quote, \
         backslash or line break. A list is one line too: `key = [\"a\", \"b\"]`.",
    );
    body.push_str("\n## Keys\n\n");
    body.push_str(&key_table(settings::KEYS));
    body.push_str("\n## The tasks table\n\n");
    body.push_str(&wrap(
        "The `[tasks]` table names the commands `fab run` uses for each step of a tracked task. Each is \
         split on whitespace and run without a shell in the task's worktree, after `{id}`, `{evidence}` \
         and `{file}` in a word are replaced. A replacement stays one argument, whatever it holds. Each \
         command must use the placeholders its row names.",
    ));
    body.push('\n');
    body.push_str(&key_table(settings::TASK_KEYS));
    let description = "The keys of .github/fabrial.toml, the settings the workflows, the review gate and \
                       the skills read.";
    Page {
        file: "settings.md",
        text: frontmatter(
            "Settings file",
            Some("fabrial.toml"),
            description,
            "Files",
            FILE_PAGES_ORDER,
        ) + &body,
    }
}

/// The page for `.github/fabrial-gates.toml`.
fn gates_page() -> Page {
    let mut body = wrap(
        "`.github/fabrial-gates.toml` says which checks cover which paths, so `fab gates` runs only the \
         checks a change touches. It runs each scope whose `paths` match a changed file. Documentation and \
         planning records run nothing unless a scope names them. The repository owns the file: `fab init` \
         writes a starter when there is none, and `fab update` and `fab check` leave it alone. Without the \
         file, `fab gates` runs the `gates` command from `.github/fabrial.toml`.",
    );
    body.push_str("\n## Keys\n\n");
    body.push_str(&key_table(gates::KEYS));
    body.push_str("\n## Scopes\n\n");
    body.push_str(&wrap(
        "Each `[[scoped]]` table is one part of the repository and the command that checks it.",
    ));
    body.push('\n');
    body.push_str(&key_table(gates::SCOPE_KEYS));
    body.push_str("\n## Starter\n\n");
    body.push_str(&wrap(
        "`fab init` writes this, with the `gates` command it was given as `full`, here `just check`:",
    ));
    let _ = write!(
        body,
        "\n```toml title=\".github/fabrial-gates.toml\"\n{}```\n",
        Gates::starter("just check")
    );
    let description = "The keys of .github/fabrial-gates.toml, which says which checks fab gates runs for \
                       the paths a change touches.";
    let order = FILE_PAGES_ORDER + 1;
    Page {
        file: "gates.md",
        text: frontmatter("Gates file", Some("fabrial-gates.toml"), description, "Files", order) + &body,
    }
}

/// The page for `.github/fabrial-sequence.toml`, with the stages this
/// release installs.
fn sequence_page() -> Result<Page> {
    let mut body = wrap(
        "`.github/fabrial-sequence.toml` lists the review stages the `fab-storm` skill runs on a pull \
         request, and `fab sequence` prints the ones the pull request's review depth gives. Each stage \
         runs once, in order, in a fresh agent. `fab sequence` reads the file from the merge base with \
         `main`, so a change to it applies to the pull requests after the one that makes it. `fab init` \
         installs the file and `fab update` refreshes it.",
    );
    body.push_str("\n## Keys\n\n");
    body.push_str(&key_table(sequence::KEYS));
    body.push_str("\n## Stages\n\n");
    body.push_str(&wrap(
        "Each `[[stage]]` table is one stage: a fresh agent in a role, run once.",
    ));
    body.push('\n');
    body.push_str(&key_table(sequence::STAGE_KEYS));
    body.push_str("\n## Finish\n\n");
    body.push_str(&wrap(
        "The `[finish]` table is the last step, after every stage the depth gives.",
    ));
    body.push('\n');
    body.push_str(&key_table(sequence::FINISH_KEYS));
    body.push_str("\n## Installed stages\n\n");
    body.push_str(&wrap(
        "The file fabrial installs runs these stages. A pull request of standard depth runs the first two, \
         and one of full depth runs all four.",
    ));
    body.push('\n');
    let installed = sequence::installed()?;
    let rows = installed
        .stage
        .iter()
        .map(|s| {
            let when = s.when.clone().unwrap_or_else(|| "always".into());
            vec![
                s.name.clone(),
                format!("`{}`", s.role),
                format!("`{}`", s.depth.name()),
                when,
            ]
        })
        .collect();
    body.push_str(&table(&["Stage", "Role", "Depth", "Runs when"], rows));
    let description = "The keys of .github/fabrial-sequence.toml, the review stages a pull request runs, \
                       and the stages fabrial installs.";
    let order = FILE_PAGES_ORDER + 2;
    Ok(Page {
        file: "sequence.md",
        text: frontmatter(
            "Sequence file",
            Some("fabrial-sequence.toml"),
            description,
            "Files",
            order,
        ) + &body,
    })
}

/// The page listing each skill `fab init` installs.
fn skills_page() -> Result<Page> {
    let mut body = wrap(
        "`fab init` installs these skills into `.claude/skills/`, and links `.agents/skills` there for \
         Codex and Pi. Claude Code offers each as a slash command of the same name, such as \
         `/fab-implement`. A skill that takes arguments names them after its description.",
    );
    for text in install::skills() {
        let field = |key: &str| frontmatter_field(text, key);
        let name = field("name").ok_or("fab manual: a skill has no `name` in its frontmatter.")?;
        let description = field("description").ok_or(format!("fab manual: skill {name} has no `description`."))?;
        let _ = write!(body, "\n## {name}\n\n{}", wrap(&description));
        if let Some(hint) = field("argument-hint") {
            let _ = write!(body, "\n{}", wrap(&format!("Arguments: `{hint}`.")));
        }
    }
    let description = "The skills fab init installs, with what each does and the arguments it takes.";
    Ok(Page {
        file: "skills.md",
        text: frontmatter("Skills", None, description, "Skills", FILE_PAGES_ORDER + 3) + &body,
    })
}

/// Appends a command's section at heading `level`: what it does, its usage,
/// its options, then a section for each visible subcommand one level down,
/// or at the same level past `###`.
fn command_section(out: &mut String, command: &Command, level: usize) {
    let name = command.get_bin_name().unwrap_or(command.get_name());
    let _ = write!(out, "\n{} {name}\n\n", "#".repeat(level));
    let about = command
        .get_long_about()
        .or(command.get_about())
        .map(|a| a.to_string())
        .unwrap_or_default();
    for paragraph in about.split("\n\n").filter(|p| !p.trim().is_empty()) {
        let _ = writeln!(out, "{}", wrap(&sentence(paragraph)));
    }
    let usage = command.clone().render_usage().to_string();
    let usage = usage.trim().strip_prefix("Usage: ").unwrap_or(usage.trim());
    let _ = write!(out, "```text\n{usage}\n```\n");
    let rows: Vec<Vec<String>> = command
        .get_arguments()
        .filter(|a| shown(a))
        .map(|a| vec![option(a), default(a), describe(a)])
        .collect();
    if !rows.is_empty() {
        out.push('\n');
        out.push_str(&table(&["Option", "Default", "Description"], rows));
    }
    for sub in command.get_subcommands().filter(|c| listed(c)) {
        command_section(out, sub, (level + 1).min(3));
    }
}

/// Whether a command belongs on a page: not hidden, and not the `help`
/// command clap adds.
fn listed(command: &Command) -> bool {
    !command.is_hide_set() && command.get_name() != "help"
}

/// Whether an argument belongs on the page: not hidden, and not the
/// `--help` and `--version` every command has.
fn shown(arg: &Arg) -> bool {
    use clap::ArgAction::{Help, HelpLong, HelpShort, Version};
    !arg.is_hide_set() && !matches!(arg.get_action(), Help | HelpShort | HelpLong | Version)
}

/// The option column: `-s, --long <VALUE>`, or `<VALUE>` for a positional.
fn option(arg: &Arg) -> String {
    let values = arg
        .get_value_names()
        .map(|names| names.iter().map(|n| format!("<{n}>")).collect::<Vec<_>>())
        .unwrap_or_else(|| vec![format!("<{}>", arg.get_id().as_str().to_uppercase())]);
    let values = values.join(" ");
    if arg.is_positional() {
        return format!("`{values}`");
    }
    let mut flags = Vec::new();
    if let Some(short) = arg.get_short() {
        flags.push(format!("-{short}"));
    }
    if let Some(long) = arg.get_long() {
        flags.push(format!("--{long}"));
    }
    let flags = flags.join(", ");
    if arg.get_action().takes_values() { format!("`{flags} {values}`") } else { format!("`{flags}`") }
}

/// The default column: the default value, `required`, `off` for a flag, or
/// `unset`.
fn default(arg: &Arg) -> String {
    let defaults: Vec<String> = arg
        .get_default_values()
        .iter()
        .map(|v| v.to_string_lossy().into_owned())
        .collect();
    if !arg.get_action().takes_values() {
        "off".into()
    } else if defaults.iter().any(|d| !d.is_empty()) {
        format!("`{}`", defaults.join(" "))
    } else if !defaults.is_empty() {
        "empty".into()
    } else if arg.is_required_set() {
        "required".into()
    } else {
        "unset".into()
    }
}

/// The description column: the help, then the values it takes, the
/// variable it reads, and whether it repeats.
fn describe(arg: &Arg) -> String {
    let help = arg
        .get_long_help()
        .or(arg.get_help())
        .map(|h| h.to_string())
        .unwrap_or_default();
    let mut parts = vec![sentence(&help.split_whitespace().collect::<Vec<_>>().join(" "))];
    if arg.get_action().takes_values() {
        let values: Vec<String> = arg
            .get_possible_values()
            .iter()
            .filter(|v| !v.is_hide_set())
            .map(|v| format!("`{}`", v.get_name()))
            .collect();
        if !values.is_empty() {
            parts.push(format!("Takes {}.", list(&values)));
        }
    }
    // Help that already names its variable, as `DRY_RUN=1` does, says enough.
    if let Some(env) = arg
        .get_env()
        .map(|e| e.to_string_lossy())
        .filter(|e| !help.contains(&format!("`{e}")))
    {
        parts.push(format!("Read from `{env}` when not given."));
    }
    if matches!(arg.get_action(), clap::ArgAction::Append) {
        parts.push("Give it more than once for more than one value.".into());
    }
    parts.retain(|p| !p.is_empty());
    parts.join(" ")
}

/// A Markdown table of keys: name, type, default and description.
fn key_table(keys: &[Key]) -> String {
    let rows = keys
        .iter()
        .map(|k| vec![format!("`{}`", k.name), k.kind.into(), k.default.into(), k.about.into()])
        .collect();
    table(&["Key", "Type", "Default", "Description"], rows)
}

/// A Markdown table with its columns padded to line up, as the docs site's
/// content needs, since its formatter skips Markdown.
fn table(header: &[&str], rows: Vec<Vec<String>>) -> String {
    let cell = |c: &str| c.replace('|', "\\|");
    let rows: Vec<Vec<String>> = rows.iter().map(|r| r.iter().map(|c| cell(c)).collect()).collect();
    let widths: Vec<usize> = (0..header.len())
        .map(|i| {
            rows.iter()
                .map(|r| r[i].chars().count())
                .chain([header[i].chars().count()])
                .max()
                .unwrap_or(0)
        })
        .collect();
    let line = |cells: Vec<String>| {
        let padded: Vec<String> = cells
            .iter()
            .zip(&widths)
            .map(|(c, w)| format!("{c}{}", " ".repeat(w - c.chars().count())))
            .collect();
        format!("| {} |\n", padded.join(" | "))
    };
    let mut out = line(header.iter().map(|h| h.to_string()).collect());
    out.push_str(&line(widths.iter().map(|w| "-".repeat(*w)).collect()));
    for row in rows {
        out.push_str(&line(row));
    }
    out
}

/// A page's frontmatter, for the docs site's Reference section.
fn frontmatter(title: &str, label: Option<&str>, description: &str, group: &str, order: usize) -> String {
    let mut out = format!("---\ntitle: {}\n", yaml(title));
    if let Some(label) = label {
        let _ = writeln!(out, "label: {}", yaml(label));
    }
    let _ = write!(
        out,
        "description: {}\nsection: Reference\ngroup: {}\norder: {order}\n---\n\n",
        yaml(description),
        yaml(group)
    );
    out
}

/// A YAML scalar: as it is when it reads back the same, quoted otherwise.
fn yaml(value: &str) -> String {
    let plain = !value.is_empty()
        && !value.contains(": ")
        && !value.contains(" #")
        && !value.starts_with(|c: char| "!&*-?|>'\"%@`[]{},#:".contains(c) || c.is_whitespace())
        && !value.ends_with([':', ' ']);
    if plain {
        value.into()
    } else {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

/// A one-line field of a skill's frontmatter, without its quotes.
fn frontmatter_field(text: &str, key: &str) -> Option<String> {
    let block = text.strip_prefix("---\n")?.split("\n---").next()?;
    let value = block
        .lines()
        .find_map(|l| l.strip_prefix(key)?.strip_prefix(':'))?
        .trim();
    let value = value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .unwrap_or(value);
    (!value.is_empty()).then(|| value.to_string())
}

/// Text ending in a full stop. clap drops the stop from a one-paragraph
/// doc comment.
fn sentence(text: &str) -> String {
    let text = text.trim();
    if text.is_empty() || text.ends_with(['.', '?', '!']) {
        return text.into();
    }
    format!("{text}.")
}

/// A list of words joined with commas and a final "or".
fn list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} or {last}", rest.join(", ")),
    }
}

/// Prose wrapped at 80 columns, ending in a line break. A code span is
/// never split.
fn wrap(text: &str) -> String {
    let mut out = String::new();
    let mut line = String::new();
    for word in spans(text) {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > 80 {
            out.push_str(&line);
            out.push('\n');
            line.clear();
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(&word);
    }
    out.push_str(&line);
    out.push('\n');
    out
}

/// The words of `text`, keeping each code span with its neighbors as one
/// word, so a wrap never breaks one.
fn spans(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut in_code = false;
    for c in text.chars() {
        if c == '`' {
            in_code = !in_code;
        }
        if c.is_whitespace() && !in_code {
            if !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
        } else {
            word.push(if c.is_whitespace() { ' ' } else { c });
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    words
}

/// Refuses a page whose prose, outside code spans and blocks, holds `<`,
/// `{` or `}`, which the docs site reads as markup, naming each such line.
fn check_markup(page: &Page) -> Result<()> {
    let body = page.text.splitn(3, "---\n").nth(2).unwrap_or(&page.text);
    let mut fenced = false;
    let mut bad = Vec::new();
    for line in body.lines() {
        if line.starts_with("```") {
            fenced = !fenced;
        } else if !fenced && line.split('`').step_by(2).any(|prose| prose.contains(['<', '{', '}'])) {
            bad.push(line.trim());
        }
    }
    if bad.is_empty() {
        return Ok(());
    }
    Err(format!(
        "fab manual: {} has `<`, `{{` or `}}` outside a code span, which the docs site reads as markup; \
         put it in backticks:\n{}",
        page.file,
        bad.join("\n")
    ))
}

#[cfg(test)]
mod tests {
    use clap::{Arg, Command};

    use super::*;

    #[test]
    fn tables_line_up_and_escape_pipes() {
        let rows = vec![vec!["`a|b`".into(), "long cell".into()], vec!["c".into(), "d".into()]];
        assert_eq!(
            table(&["Key", "Value"], rows),
            "| Key    | Value     |\n\
             | ------ | --------- |\n\
             | `a\\|b` | long cell |\n\
             | c      | d         |\n"
        );
    }

    #[test]
    fn yaml_quotes_only_what_needs_it() {
        assert_eq!(yaml("Setup commands"), "Setup commands");
        assert_eq!(yaml("fabrial.toml"), "fabrial.toml");
        assert_eq!(yaml("Runs: this"), "\"Runs: this\"");
        assert_eq!(yaml("`code` \"q\""), "\"`code` \\\"q\\\"\"");
    }

    #[test]
    fn wrap_keeps_code_spans_whole() {
        let text = format!("{} `a b c`", "word ".repeat(15).trim());
        assert_eq!(wrap(&text), format!("{}\n`a b c`\n", "word ".repeat(15).trim()));
    }

    #[test]
    fn markup_outside_code_is_refused() {
        let page = |text: &str| Page { file: "p.md", text: format!("---\ntitle: t\n---\n\n{text}") };
        assert!(check_markup(&page("`<a>` and `{id}`\n```text\nfab <X>\n```\n")).is_ok());
        let err = check_markup(&page("a storm/<task> branch\n")).unwrap_err();
        assert!(err.contains("p.md") && err.contains("storm/<task>"), "{err}");
        assert!(check_markup(&page("given {id}\n")).is_err());
    }

    #[test]
    fn a_command_on_no_page_is_refused() {
        let cli = Command::new("fab")
            .subcommand(Command::new("init"))
            .subcommand(Command::new("new-thing"));
        let err = pages(cli).err().unwrap();
        assert!(err.contains("`fab new-thing` is on no page"), "{err}");
    }

    #[test]
    fn options_show_their_default_values_and_variable() {
        let arg = Arg::new("base")
            .long("base")
            .default_value("origin/main")
            .help("The base");
        assert_eq!(option(&arg), "`--base <BASE>`");
        assert_eq!(default(&arg), "`origin/main`");
        assert_eq!(describe(&arg), "The base.");
        let arg = Arg::new("repo")
            .long("repo")
            .required(true)
            .env("REPO")
            .value_parser(["a", "b"]);
        assert_eq!(default(&arg), "required");
        assert_eq!(describe(&arg), "Takes `a` or `b`. Read from `REPO` when not given.");
        let flag = Arg::new("force").long("force").action(clap::ArgAction::SetTrue);
        assert_eq!((option(&flag), default(&flag)), ("`--force`".into(), "off".into()));
    }

    #[test]
    fn skill_frontmatter_fields_lose_their_quotes() {
        let text = "---\nname: fab-x\ndescription: Does x.\nargument-hint: \"[pr]\"\n---\n\n# X\n";
        assert_eq!(frontmatter_field(text, "name").as_deref(), Some("fab-x"));
        assert_eq!(frontmatter_field(text, "argument-hint").as_deref(), Some("[pr]"));
        assert_eq!(frontmatter_field(text, "missing"), None);
    }
}
