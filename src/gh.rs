use std::{
    ffi::{OsStr, OsString},
    io,
    process::{Command, Output},
    sync::{Mutex, OnceLock},
};

static DEBUG_OUTPUT_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub fn run_gh<I, S>(args: I, debug: bool) -> io::Result<Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let args: Vec<OsString> = args
        .into_iter()
        .map(|argument| argument.as_ref().to_os_string())
        .collect();
    let output = Command::new("gh").args(&args).output();

    if debug {
        log_command(&args, &output);
    }

    output
}

fn log_command(args: &[OsString], output: &io::Result<Output>) {
    let guard = DEBUG_OUTPUT_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("debug output lock poisoned");

    eprintln!("[debug] gh command: {}", format_command(args));

    match output {
        Ok(output) => {
            print_stream("stdout", &output.stdout);
            print_stream("stderr", &output.stderr);
        }
        Err(error) => {
            eprintln!("[debug] failed to execute gh: {error}");
        }
    }

    drop(guard);
}

fn print_stream(label: &str, bytes: &[u8]) {
    eprintln!("[debug] gh {label}:");
    if bytes.is_empty() {
        eprintln!("<empty>");
        return;
    }

    eprint!("{}", String::from_utf8_lossy(bytes));
    if !bytes.ends_with(b"\n") {
        eprintln!();
    }
}

fn format_command(args: &[OsString]) -> String {
    std::iter::once("gh".to_string())
        .chain(
            args.iter()
                .map(|argument| quote_argument(&argument.to_string_lossy())),
        )
        .collect::<Vec<_>>()
        .join(" ")
}

fn quote_argument(argument: &str) -> String {
    if !argument.is_empty()
        && argument.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | '/' | '\\')
        })
    {
        argument.to_string()
    } else {
        format!("'{}'", argument.replace('\'', "''"))
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::format_command;

    #[test]
    fn leaves_simple_arguments_unquoted() {
        let command = format_command(&[
            OsString::from("api"),
            OsString::from("user"),
            OsString::from("--jq"),
            OsString::from(".login"),
        ]);

        assert_eq!(command, "gh api user --jq .login");
    }

    #[test]
    fn quotes_arguments_with_powershell_sensitive_characters() {
        let command = format_command(&[
            OsString::from("api"),
            OsString::from("-H"),
            OsString::from("Accept: application/vnd.github+json"),
            OsString::from("/users/kevin/settings/billing/premium_request/usage?year=2026&month=3"),
        ]);

        assert_eq!(
            command,
            "gh api -H 'Accept: application/vnd.github+json' '/users/kevin/settings/billing/premium_request/usage?year=2026&month=3'"
        );
    }
}
