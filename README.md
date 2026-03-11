# copilot-usage

`copilot-usage` is a Rust CLI tool that shows how fast you're consuming GitHub Copilot premium requests and whether that pace is sustainable for the rest of the month.

<img width="2314" height="1521" alt="image" src="https://github.com/user-attachments/assets/f2ba0578-e31f-4c13-bf15-adc63ab5160e" />

It fetches billing data through your logged-in `gh` session, then prints a compact terminal dashboard with:

- month-to-date usage and remaining quota
- average daily pace
- a "today budget" progress bar
- projected month-end cushion or overshoot
- optional `--full` history details

## Requirements

- Rust and Cargo
- GitHub CLI (`gh`)
- a GitHub account with access to Copilot premium request billing data

## Install

### Install with cargo

```powershell
cargo install copilot-usage
```

### Build from a local checkout

```powershell
git clone https://github.com/kevingosse/copilot-usage.git
cd copilot-usage
cargo build --release
```

The binary will be available at:

```text
target\release\copilot-usage.exe
```

### Install into Cargo's bin directory

```powershell
git clone https://github.com/kevingosse/copilot-usage.git
cd copilot-usage
cargo install --path .
```

After that, you can run `copilot-usage` directly from your shell.

## Authenticate with `gh`

Log in once:

```powershell
gh auth login
```

If the billing endpoint complains about missing scopes, refresh the `user` scope:

```powershell
gh auth refresh -h github.com -s user
```

You can verify your login with:

```powershell
gh auth status
```

## Usage

Run the default view:

```powershell
copilot-usage
```

Show the full dashboard with recent daily detail and previous months:

```powershell
copilot-usage --full
```

Override the username or quota if needed:

```powershell
copilot-usage --username kevingosse --quota 1500
```

## Options

- `--username <USER>`: GitHub username. If omitted, the tool tries `GITHUB_USER`, `GH_USERNAME`, then `gh api user --jq .login`
- `--quota <N>`: monthly quota override, default `1500`
- `--months <N>`: number of previous months to compare in `--full`, default `6`
- `--full`: show the full dashboard with sparkline, previous months, and model breakdown
