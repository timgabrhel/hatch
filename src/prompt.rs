use crate::Result;
use console::{style, Term};
use dialoguer::{theme::ColorfulTheme, Input, Select};
use std::io::{self, IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};
static NAVIGATION_SHOWN: AtomicBool = AtomicBool::new(false);
static PLAIN: AtomicBool = AtomicBool::new(false);
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
    eprintln!("\n  {}", style(clean(title)).bold());
    for (key, value) in fields {
        eprintln!("  {:<11} {}", clean(key), clean(value));
    }
    eprintln!();
}
pub fn input(title: &str) -> Result<String> {
    if !plain() {
        note("Paste a path, then press Enter. Leave it blank to go back.");
        return Input::<String>::with_theme(&ColorfulTheme::default())
            .with_prompt(title)
            .allow_empty(true)
            .report(false)
            .interact_on(&Term::stderr())
            .map_err(|e| format!("path input: {e}"));
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
        eprintln!("  {}\n", style("Step inside. Get to work.").dim());
    }
}
pub fn confirm(title: &str) -> Result<bool> {
    if plain() {
        eprint!("{} [y/N] ", clean(title));
        io::stderr().flush().map_err(|e| e.to_string())?;
        let mut answer = String::new();
        io::stdin()
            .read_line(&mut answer)
            .map_err(|e| e.to_string())?;
        return Ok(matches!(
            answer.trim().to_ascii_lowercase().as_str(),
            "y" | "yes"
        ));
    }
    Ok(choose_default(
        title,
        &["Not now".into(), "Install inside this container".into()],
        true,
        Some(0),
    )? == 1)
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
        let safe_labels: Vec<String> = labels.iter().map(|s| clean(s)).collect();
        if !NAVIGATION_SHOWN.swap(true, Ordering::Relaxed) {
            eprintln!(
                "  {}",
                style("↑/↓ navigate · Enter select · Esc cancel/back").dim()
            );
        }
        return Select::with_theme(&ColorfulTheme::default())
            .with_prompt(clean(title))
            .items(&safe_labels)
            .default(default.unwrap_or(0))
            .report(false)
            .max_length(8)
            .interact_on_opt(&Term::stderr())
            .map_err(|e| format!("menu input: {e}"))?
            .ok_or_else(|| "cancelled; nothing launched".into());
    }
    eprintln!("{title}");
    for (i, label) in labels.iter().enumerate() {
        // Debug escaping prevents container-controlled terminal escape sequences.
        let suffix = if default == Some(i) { " (default)" } else { "" };
        eprintln!("  {}) {:?}{}", i + 1, label, suffix);
    }
    loop {
        if let Some(i) = default {
            eprint!(
                "Choose 1-{} [Enter = {}] (q cancels): ",
                labels.len(),
                i + 1
            );
        } else {
            eprint!("Choose 1-{} (q cancels): ", labels.len());
        }
        io::stderr().flush().map_err(|e| e.to_string())?;
        let mut line = String::new();
        let n = io::stdin()
            .read_line(&mut line)
            .map_err(|e| e.to_string())?;
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
