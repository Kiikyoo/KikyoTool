use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[derive(Debug, Clone)]
pub struct CommandOutcome {
    pub program: String,
    pub args: Vec<String>,
    pub status_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
}

impl CommandOutcome {
    pub fn failed(program: &str, args: &[String], message: impl Into<String>) -> Self {
        Self {
            program: sanitize_program(program),
            args: args.to_vec(),
            status_code: None,
            stdout: String::new(),
            stderr: message.into(),
            success: false,
        }
    }

    pub fn command_line(&self) -> String {
        std::iter::once(quote_arg(&self.program))
            .chain(self.args.iter().map(|arg| quote_arg(arg)))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

pub fn run_process(program: &str, args: &[String]) -> CommandOutcome {
    let program = sanitize_program(program);
    if program.is_empty() {
        return CommandOutcome::failed("", args, "Program path is empty.");
    }

    let mut command = Command::new(&program);
    command.args(args);

    #[cfg(target_os = "windows")]
    command.creation_flags(0x08000000);

    match command.output() {
        Ok(output) => CommandOutcome {
            program,
            args: args.to_vec(),
            status_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            success: output.status.success(),
        },
        Err(error) => CommandOutcome::failed(&program, args, error.to_string()),
    }
}

pub fn quote_command(program: &str, args: &[String]) -> String {
    std::iter::once(quote_arg(&sanitize_program(program)))
        .chain(args.iter().map(|arg| quote_arg(arg)))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn open_terminal_with_text(title: &str, text: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let path = std::env::temp_dir().join(format!(
            "kikyo_tool_output_{}_{}.txt",
            std::process::id(),
            timestamp_millis()
        ));
        fs::write(&path, text).map_err(|error| error.to_string())?;

        let path = ps_single_quote(&path.display().to_string());
        let title = ps_single_quote(title);
        let script = format!(
            "$Host.UI.RawUI.WindowTitle = {title}; \
             [Console]::OutputEncoding = [System.Text.Encoding]::UTF8; \
             if (Test-Path -LiteralPath {path}) {{ \
                 Get-Content -Raw -Encoding UTF8 -LiteralPath {path}; \
                 Remove-Item -LiteralPath {path} -Force -ErrorAction SilentlyContinue \
             }}; \
             Write-Host ''; \
             Write-Host '按 Enter 关闭窗口...'; \
             Read-Host | Out-Null"
        );

        let mut command = Command::new("powershell.exe");
        command.args([
            "-NoLogo",
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ]);
        command.creation_flags(0x00000010);
        command
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = title;
        println!("{text}");
        Ok(())
    }
}

pub fn open_cmd_with_commands(command_lines: &[&str]) -> Result<(), String> {
    if command_lines.is_empty() {
        return Err("No command lines were provided.".to_owned());
    }

    #[cfg(target_os = "windows")]
    {
        let command_line = command_lines
            .iter()
            .map(|line| {
                if line.trim_start().starts_with("conda activate ") {
                    format!("call {line}")
                } else {
                    (*line).to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join(" & ");
        let mut command = Command::new("cmd.exe");
        command.args(["/K", &command_line]);
        command.creation_flags(0x00000010);
        command
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    #[cfg(not(target_os = "windows"))]
    {
        let command_line = command_lines.join(";\n");
        let mut command = Command::new("sh");
        command.args(["-lc", &command_line]);
        command
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

fn sanitize_program(program: &str) -> String {
    program.trim().trim_matches('"').to_owned()
}

fn timestamp_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn ps_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn quote_arg(value: &str) -> String {
    if value.is_empty() {
        return "\"\"".to_owned();
    }

    if value
        .chars()
        .any(|ch| ch.is_whitespace() || matches!(ch, '"' | '\'' | '&' | '(' | ')' | ';'))
    {
        format!("\"{}\"", value.replace('"', "\\\""))
    } else {
        value.to_owned()
    }
}
