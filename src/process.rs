use crate::Result;
use std::process::{Command, Stdio};

pub fn capture(command: &mut Command, operation: &str) -> Result<Vec<u8>> {
    let output = command.stdin(Stdio::null()).output().map_err(|e| {
        format!("{operation}: {e}. Check that the executable is installed and available on PATH.")
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let diagnostic = [stderr.trim(), stdout.trim()]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        return Err(format!(
            "{operation} failed ({}): {}",
            output.status,
            if diagnostic.is_empty() {
                "no diagnostic output"
            } else {
                &diagnostic
            }
        ));
    }
    Ok(output.stdout)
}
