use crate::Result;
use console::{style, Key, Term};
use dialoguer::{
    theme::{ColorfulTheme, Theme},
    Select,
};
use std::io::{self, IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};
static NAVIGATION_SHOWN: AtomicBool = AtomicBool::new(false);
static PLAIN: AtomicBool = AtomicBool::new(false);
pub fn path_item(title: &str, path: &str) -> String {
    format!("{}\nPath: {}", clean(title), clean(path))
}
struct MenuTheme;
impl Theme for MenuTheme {
    fn format_select_prompt(&self, f: &mut dyn std::fmt::Write, prompt: &str) -> std::fmt::Result {
        // Keep the spacer inside the menu's redraw area so it disappears with
        // completed menus instead of accumulating between selection steps.
        writeln!(f)?;
        ColorfulTheme::default().format_select_prompt(f, prompt)
    }
    fn format_select_prompt_item(
        &self,
        f: &mut dyn std::fmt::Write,
        text: &str,
        active: bool,
    ) -> std::fmt::Result {
        let (title, detail) = text.split_once('\n').unwrap_or((text, ""));
        if active {
            write!(f, "{} {}", style("❯").cyan(), style(title).cyan().bold())?;
        } else {
            write!(f, "  {title}")?;
        }
        if !detail.is_empty() {
            write!(f, "\n    {}", style(detail).dim())?;
        }
        Ok(())
    }
}
pub fn set_plain(value: bool) {
    PLAIN.store(value, Ordering::Relaxed);
}
pub fn plain() -> bool {
    PLAIN.load(Ordering::Relaxed)
        || std::env::var_os("HATCH_PLAIN").is_some()
        || std::env::var("TERM").as_deref() == Ok("dumb")
}
pub fn clean(value: &str) -> String {
    value
        .chars()
        .flat_map(|c| {
            if c.is_control() {
                c.escape_default().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect()
}
pub fn warning(value: &str) {
    eprintln!("\n  {}", style("Notice").yellow().bold());
    note(value);
}
pub fn note(value: &str) {
    let width = usize::from(Term::stderr().size().1).clamp(40, 100) - 4;
    for paragraph in value.lines() {
        let mut line = String::new();
        for word in clean(paragraph).split_whitespace() {
            if !line.is_empty()
                && console::measure_text_width(&line) + console::measure_text_width(word) + 1
                    > width
            {
                eprintln!("  {line}");
                line.clear();
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        eprintln!("  {line}");
    }
}
pub fn section(title: &str, fields: &[(&str, &str)]) {
    eprint!("{}", section_text(title, fields));
}
fn section_text(title: &str, fields: &[(&str, &str)]) -> String {
    use std::fmt::Write;
    let mut output = String::new();
    let title = clean(title);
    if plain() {
        writeln!(&mut output, "\n  {title}").unwrap();
    } else {
        writeln!(&mut output, "\n  {}", style(title).cyan().bold()).unwrap();
    }
    let width = fields
        .iter()
        .map(|(key, _)| clean(key).len() + 1)
        .max()
        .unwrap_or(0);
    for (key, value) in fields {
        let label = format!("{:<width$}", format!("{}:", clean(key)));
        if plain() {
            writeln!(&mut output, "  {label} {}", clean(value)).unwrap();
        } else {
            writeln!(&mut output, "  {} {}", style(label).dim(), clean(value)).unwrap();
        }
    }
    output
}
/// Display host paths relative to the invocation directory. Actual resolver and
/// Docker arguments remain canonical/absolute; container paths are not host paths.
pub fn local_path(path: &std::path::Path) -> String {
    if !path.is_absolute() {
        return path.display().to_string();
    }
    let Ok(base) = std::env::current_dir() else {
        return "(current directory unavailable)".into();
    };
    relative_path(path, &base)
}

fn relative_path(path: &std::path::Path, base: &std::path::Path) -> String {
    let parts = path.components().collect::<Vec<_>>();
    let root = base.components().collect::<Vec<_>>();
    let common = parts.iter().zip(&root).take_while(|(a, b)| a == b).count();
    let mut relative = std::path::PathBuf::new();
    for _ in &root[common..] {
        relative.push("..");
    }
    for part in &parts[common..] {
        relative.push(part.as_os_str());
    }
    if relative.as_os_str().is_empty() {
        ".".into()
    } else {
        relative.display().to_string()
    }
}
pub fn input(title: &str) -> Result<String> {
    if io::stdin().is_terminal() && io::stderr().is_terminal() {
        let term = Term::stderr();
        let read = || -> io::Result<String> {
            term.write_str(&format!(
                "{} (Esc, empty Enter or q goes back): ",
                clean(title)
            ))?;
            let mut text = String::new();
            loop {
                // console restores terminal attributes after each read, including
                // errors. Read keys explicitly: read_line silently ignores Escape.
                match term.read_key_raw()? {
                    Key::Escape | Key::CtrlC => {
                        term.write_line("")?;
                        return Ok(String::new());
                    }
                    Key::Enter => {
                        term.write_line("")?;
                        return Ok(text.trim().to_owned());
                    }
                    Key::Backspace => {
                        if let Some(ch) = text.pop() {
                            term.clear_chars(console::measure_text_width(&ch.to_string()))?;
                        }
                    }
                    Key::Char(ch) if !ch.is_control() => {
                        text.push(ch);
                        term.write_str(&ch.to_string())?;
                    }
                    _ => {}
                }
            }
        };
        return read().map_err(|e| format!("path input: {e}"));
    }
    eprint!("{} (Enter or q goes back): ", clean(title));
    io::stderr().flush().map_err(|e| e.to_string())?;
    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    Ok(line.trim().into())
}
pub fn heading() {
    if !plain() {
        let wordmark = r"   _           _       _
  | |__   __ _| |_ ___| |__
  | '_ \ / _` | __/ __| '_ \
  | | | | (_| | || (__| | | |
  |_| |_|\__,_|\__\___|_| |_|";
        if Term::stderr().size().1 >= 36 {
            eprintln!("\n{}", style(wordmark).cyan().bold());
        } else {
            eprintln!("\n  {}", style("HATCH").cyan().bold());
        }
        eprintln!("  {}", style("Step inside. Get to work.").dim());
        eprintln!("  {}", style(concat!("v", env!("CARGO_PKG_VERSION"))).dim());
    }
}

pub fn workflow(title: &str, description: &str) {
    heading();
    eprintln!("\n  {}", style(clean(title)).bold());
    note(description);
}
/// The review belongs to the menu's redraw area, so Back removes the old
/// summary before the preceding selection is rendered again.
pub fn review(fields: &[(&str, &str)], install: bool) -> Result<bool> {
    let action = if install {
        "Install Agent & Launch"
    } else {
        "Confirm and launch"
    };
    let labels = vec![action.into()];
    if plain() {
        // Numbered mode preserves its transcript. Reuse an unchanged summary
        // when returning from Back instead of appending the same block again.
        static LAST_REVIEW: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
        let summary = section_text("Your session", fields);
        let mut last = LAST_REVIEW.lock().unwrap();
        if *last != summary {
            eprint!("{summary}");
            *last = summary;
        }
        drop(last);
        return choose_default("Launch this session?", &labels, true, Some(0)).map(|i| i == 0);
    }
    navigation();
    let theme = ReviewTheme {
        summary: review_text(fields, install),
    };
    Select::with_theme(&theme)
        .with_prompt("Launch this session?")
        .items(&labels)
        .default(0)
        .report(false)
        .interact_on_opt(&Term::stderr())
        .map(|choice| choice == Some(0))
        .map_err(|e| format!("menu input: {e}"))
}
fn review_text(fields: &[(&str, &str)], install: bool) -> String {
    use std::fmt::Write;
    // Explicit wrapping lets dialoguer count every physical review row when it
    // clears the prompt. Leave the last column free to avoid terminal autowrap.
    let columns = usize::from(Term::stderr().size().1).max(16) - 1;
    let label_width = fields
        .iter()
        .map(|(key, _)| clean(key).len() + 1)
        .max()
        .unwrap_or(0)
        .min(columns / 2);
    let mut output = format!("\n  {}\n", style("Your session").cyan().bold());
    for (key, value) in fields {
        let needs_install = install && *key == "Agent status";
        let key = format!("{}:", clean(key));
        let label = format!(
            "{:<label_width$}",
            console::truncate_str(&key, label_width, "…")
        );
        write!(output, "  {} ", style(label).dim()).unwrap();
        let mut used = label_width + 3;
        let mut rendered = String::new();
        for ch in clean(value).chars() {
            let width = console::measure_text_width(&ch.to_string());
            if used + width > columns {
                rendered.push_str("\n    ");
                used = 4;
            }
            rendered.push(ch);
            used += width;
        }
        if needs_install {
            writeln!(output, "{}", style(rendered).yellow()).unwrap();
        } else {
            writeln!(output, "{rendered}").unwrap();
        }
    }
    output
}
struct ReviewTheme {
    summary: String,
}
impl Theme for ReviewTheme {
    fn format_select_prompt(&self, f: &mut dyn std::fmt::Write, prompt: &str) -> std::fmt::Result {
        write!(f, "{}", self.summary)?;
        MenuTheme.format_select_prompt(f, prompt)
    }
    fn format_select_prompt_item(
        &self,
        f: &mut dyn std::fmt::Write,
        text: &str,
        active: bool,
    ) -> std::fmt::Result {
        MenuTheme.format_select_prompt_item(f, text, active)
    }
}
fn navigation() {
    if !NAVIGATION_SHOWN.swap(true, Ordering::Relaxed) {
        eprintln!(
            "\n  {}",
            style("↑/↓ navigate · Enter select · Esc back").dim()
        );
    }
}

pub fn available(no_input: bool) -> bool {
    !no_input && io::stdin().is_terminal() && io::stderr().is_terminal()
}

pub fn choose(title: &str, labels: &[String], enabled: bool) -> Result<usize> {
    choose_default(title, labels, enabled, None)
}

pub fn choose_default(
    title: &str,
    labels: &[String],
    enabled: bool,
    default: Option<usize>,
) -> Result<usize> {
    if !enabled {
        return Err(format!("{title}: selection requires a terminal; specify --container, --workspace and/or --user explicitly"));
    }
    if labels.is_empty() {
        return Err(format!("{title}: no choices available"));
    }
    if !plain() {
        let safe_labels: Vec<String> = labels
            .iter()
            .map(|s| s.lines().map(clean).collect::<Vec<_>>().join("\n"))
            .collect();
        navigation();
        return Select::with_theme(&MenuTheme)
            .with_prompt(clean(title))
            .items(&safe_labels)
            .default(default.unwrap_or(0))
            .report(false)
            .max_length(if labels.iter().any(|label| label.contains('\n')) {
                5
            } else {
                labels.len()
            })
            .interact_on_opt(&Term::stderr())
            .map_err(|e| format!("menu input: {e}"))?
            .ok_or_else(|| "cancelled; nothing launched".into());
    }
    eprintln!("\n{title}");
    for (i, label) in labels.iter().enumerate() {
        // Debug escaping prevents container-controlled terminal escape sequences.
        let suffix = if default == Some(i) { " (default)" } else { "" };
        let mut lines = label.lines();
        eprintln!(
            "  {}) {}{}",
            i + 1,
            clean(lines.next().unwrap_or_default()),
            suffix
        );
        for detail in lines {
            eprintln!("     {}", clean(detail));
        }
    }
    loop {
        if let Some(i) = default {
            eprint!(
                "Choose 1-{} [Enter = {}] (Esc/q goes back): ",
                labels.len(),
                i + 1
            );
        } else {
            eprint!("Choose 1-{} (Esc/q goes back): ", labels.len());
        }
        io::stderr().flush().map_err(|e| e.to_string())?;
        let mut line = String::new();
        let n = if io::stdin().is_terminal() && io::stderr().is_terminal() {
            let term = Term::stderr();
            loop {
                match term.read_key_raw().map_err(|e| e.to_string())? {
                    Key::Escape | Key::CtrlC => return Err("cancelled; nothing launched".into()),
                    Key::Enter => {
                        eprintln!();
                        break;
                    }
                    Key::Backspace => {
                        if line.pop().is_some() {
                            term.clear_chars(1).map_err(|e| e.to_string())?;
                        }
                    }
                    Key::Char(ch) if !ch.is_control() => {
                        line.push(ch);
                        term.write_str(&ch.to_string()).map_err(|e| e.to_string())?;
                    }
                    _ => {}
                }
            }
            1
        } else {
            io::stdin()
                .read_line(&mut line)
                .map_err(|e| e.to_string())?
        };
        let value = line.trim();
        if n == 0 || value.eq_ignore_ascii_case("q") {
            return Err("cancelled; nothing launched".into());
        }
        if value.is_empty() {
            if let Some(i) = default {
                return Ok(i);
            }
        }
        if let Ok(i) = value.parse::<usize>() {
            if i > 0 && i <= labels.len() {
                return Ok(i - 1);
            }
        }
        eprintln!("Enter a listed number or q.");
    }
}

#[cfg(test)]
mod tests {
    use super::relative_path;
    use std::path::Path;
    #[test]
    fn local_display_paths_are_relative_including_parent_projects() {
        let base = Path::new("/projects/hatch");
        for (path, expected) in [
            ("/projects/hatch", "."),
            (
                "/projects/hatch/.devcontainer/devcontainer.json",
                ".devcontainer/devcontainer.json",
            ),
            (
                "/projects/hatch/Hatch.code-workspace",
                "Hatch.code-workspace",
            ),
            (
                "/projects/other/devcontainer.json",
                "../other/devcontainer.json",
            ),
        ] {
            assert_eq!(relative_path(Path::new(path), base), expected);
        }
    }
}
