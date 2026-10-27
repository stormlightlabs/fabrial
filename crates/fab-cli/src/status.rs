//! The status comment: one bot comment per pull request that holds its
//! state in named sections, such as the standard review's findings and the
//! squash message. Writers edit their own section in place, and GitHub sends
//! no notification for an edit, so the owner hears of the comment once,
//! when it is first posted, instead of once per update.
//!
//! The comment opens with [`MARKER`]. Each section starts with a line
//! `<!-- fabrial:section <name> -->` and runs to the next such line. Only a
//! comment from the bot counts, so a copy someone else posts is ignored.

use std::path::Path;

use clap::Args;
use clap::builder::FalseyValueParser;

use crate::run::{self, Result};

/// The first line of the status comment.
pub const MARKER: &str = "<!-- fabrial:status -->";
/// The bot's login as the REST API gives it.
const BOT_LOGIN: &str = "stormlight-thunderbot[bot]";
const SECTION_OPEN: &str = "<!-- fabrial:section ";
const SECTION_CLOSE: &str = " -->";

/// What `fab pr-status` writes, each also read from its environment
/// variable.
#[derive(Args)]
pub struct Options {
    /// The repository, as owner/name.
    #[arg(long, env = "REPO")]
    repo: String,
    /// The pull request number.
    #[arg(long, env = "PR")]
    pr: String,
    /// The section to set, such as `Standard pass`.
    #[arg(long, env = "SECTION")]
    section: String,
    /// A file holding the section's text.
    #[arg(long)]
    body_file: std::path::PathBuf,
    /// Print the write instead of sending it. `DRY_RUN=1` sets it.
    #[arg(long, env = "DRY_RUN", action = clap::ArgAction::SetTrue, value_parser = FalseyValueParser::new())]
    dry_run: bool,
}

/// A pull request's status comment as it stands, or none yet.
pub struct Status {
    repo: String,
    pr: String,
    id: Option<u64>,
    sections: Vec<(String, String)>,
}

impl Status {
    /// Reads the bot's status comment on a pull request. A pull request
    /// with none has no sections.
    pub fn read(repo: &str, pr: &str) -> Result<Status> {
        let path = format!("repos/{repo}/issues/{pr}/comments?per_page=100");
        // One line per comment, the id then the body as a JSON string, so a
        // body's own line breaks cannot split the answer.
        let filter = format!(
            ".[] | select(.user.login == \"{BOT_LOGIN}\" and (.body | startswith(\"{MARKER}\"))) \
             | \"\\(.id) \\(.body | tojson)\""
        );
        let found = run::gh(&["api", "--paginate", &path, "--jq", &filter])?;
        let (id, body) = match found.lines().find(|l| !l.trim().is_empty()) {
            Some(line) => {
                let bad = || format!("cannot read the status comment from `{line}`");
                let (id, body) = line.split_once(' ').ok_or_else(bad)?;
                let body: String = serde_json::from_str(body).map_err(|_| bad())?;
                (Some(id.parse::<u64>().map_err(|_| bad())?), body)
            }
            None => (None, String::new()),
        };
        Ok(Status { repo: repo.into(), pr: pr.into(), id, sections: parse(&body) })
    }

    /// A section's text, if the comment has it.
    pub fn section(&self, name: &str) -> Option<&str> {
        self.sections.iter().find(|(n, _)| n == name).map(|(_, t)| t.as_str())
    }

    /// Sets a section and writes the comment: an edit when it exists, else a
    /// new comment. Writes nothing and returns false when the section
    /// already says the same. With `dry_run`, prints the write instead.
    pub fn set(&mut self, name: &str, text: &str, scratch: &Path, dry_run: bool) -> Result<bool> {
        let text = escape(text);
        let text = text.trim_matches('\n');
        if self.section(name) == Some(text) {
            return Ok(false);
        }
        match self.sections.iter_mut().find(|(n, _)| n == name) {
            Some((_, old)) => *old = text.to_string(),
            None => self.sections.push((name.to_string(), text.to_string())),
        }
        let file = scratch.join("status.md");
        std::fs::write(&file, render(&self.sections)).map_err(|e| format!("cannot write the status comment: {e}"))?;
        let body = format!("body=@{}", file.display());
        let path = match self.id {
            Some(id) => format!("repos/{}/issues/comments/{id}", self.repo),
            None => format!("repos/{}/issues/{}/comments", self.repo, self.pr),
        };
        let method = if self.id.is_some() { "PATCH" } else { "POST" };
        let args = ["api", "-X", method, &path, "-F", &body];
        if dry_run {
            run::print_write(&args);
            eprintln!("DRY_RUN body: {:?}", render(&self.sections));
            return Ok(true);
        }
        let posted = run::gh(&[&args[..], &["--jq", ".id"]].concat())?;
        self.id = self.id.or_else(|| posted.trim().parse().ok());
        Ok(true)
    }
}

/// Sets one section of a pull request's status comment, for the Bot comment
/// workflow.
pub fn run(options: Options) -> Result<()> {
    let text = std::fs::read_to_string(&options.body_file)
        .map_err(|e| format!("cannot read {}: {e}", options.body_file.display()))?;
    if !is_section_name(&options.section) {
        return Err(format!(
            "`{}` is not a section name: use letters, digits and spaces.",
            options.section
        ));
    }
    let scratch = tempfile::tempdir().map_err(|e| format!("cannot make a scratch directory: {e}"))?;
    let mut status = Status::read(&options.repo, &options.pr)?;
    if !status.set(&options.section, &text, scratch.path(), options.dry_run)? {
        println!("The {} section already says that.", options.section);
    }
    Ok(())
}

/// Whether a name can head a section without breaking the comment: letters,
/// digits and inner spaces.
fn is_section_name(name: &str) -> bool {
    !name.is_empty() && name.trim() == name && name.chars().all(|c| c.is_ascii_alphanumeric() || c == ' ')
}

/// A section's text with a space before any line that would read as the
/// start of a section or of the comment, so text cannot split the comment.
fn escape(text: &str) -> String {
    text.replace('\r', "")
        .lines()
        .map(|l| if l.starts_with(SECTION_OPEN) || l.starts_with(MARKER) { format!(" {l}") } else { l.to_string() })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The sections of a status comment's body, in order.
fn parse(body: &str) -> Vec<(String, String)> {
    let mut sections: Vec<(String, Vec<&str>)> = Vec::new();
    let body = body.replace('\r', "");
    for line in body.lines() {
        let name = line
            .strip_prefix(SECTION_OPEN)
            .and_then(|rest| rest.strip_suffix(SECTION_CLOSE));
        match (name, sections.last_mut()) {
            (Some(name), _) => sections.push((name.to_string(), Vec::new())),
            (None, Some((_, lines))) => lines.push(line),
            (None, None) => {}
        }
    }
    sections
        .into_iter()
        .map(|(name, lines)| (name, lines.join("\n").trim_matches('\n').to_string()))
        .collect()
}

/// A status comment's body from its sections.
fn render(sections: &[(String, String)]) -> String {
    let mut out = format!("{MARKER}\n");
    for (name, text) in sections {
        out.push_str(&format!("{SECTION_OPEN}{name}{SECTION_CLOSE}\n{text}\n\n"));
    }
    out.trim_end_matches('\n').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_read_back_in_order() {
        let sections = vec![
            (
                "Standard pass".to_string(),
                "Standard pass on abc · m\n\nNo findings".to_string(),
            ),
            (
                "Squash message".to_string(),
                "When you merge:\n\n```text\nX (#1)\n```".to_string(),
            ),
        ];
        let body = render(&sections);
        assert!(body.starts_with(MARKER));
        assert_eq!(parse(&body), sections);
        assert_eq!(parse(&body.replace('\n', "\r\n")), sections);
    }

    #[test]
    fn text_outside_a_section_is_dropped() {
        let body = format!("{MARKER}\nloose\n<!-- fabrial:section A -->\na\n");
        assert_eq!(parse(&body), vec![("A".to_string(), "a".to_string())]);
        assert_eq!(parse(""), vec![]);
    }

    #[test]
    fn text_cannot_open_a_section() {
        let text = escape("a\n<!-- fabrial:section Squash message -->\nb");
        let body = render(&[("A".to_string(), text.clone())]);
        assert_eq!(parse(&body), vec![("A".to_string(), text)]);
    }

    #[test]
    fn section_names_cannot_break_the_comment() {
        assert!(is_section_name("Standard pass"));
        for name in ["", " A", "A -->", "A\nB", "<!--"] {
            assert!(!is_section_name(name), "{name:?}");
        }
    }
}
