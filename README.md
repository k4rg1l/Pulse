# Pulse

Everything you care about, in one tray app.

Pulse sits in your Windows tray and shows your computer, your Claude limits and your Codex limits in one panel, one click or one shortcut from anywhere.

![Pulse showing System Resources, Codex and Claude](docs/screenshots/hero.jpg)

## Download

**[Download Pulse for Windows](https://github.com/k4rg1l/Pulse/releases/latest/download/Pulse-setup.exe)**

Installing needs no admin rights. Pulse opens by itself the first time, then lives in your tray. Press `Ctrl` `Alt` `P` to open it from anywhere.

Windows may show a SmartScreen notice because Pulse is a new app. Choose **More info**, then **Run anyway**. You can check your download against its [SHA-256 checksum](https://github.com/k4rg1l/Pulse/releases/latest/download/Pulse-setup.exe.sha256).

## What it shows

### System Resources

CPU, every core, memory, disks and network, all live. GPU usage, memory and temperature appear on NVIDIA graphics cards. Any number can sit on the tray icon itself.

### Claude

Your 5-hour and weekly limits, tokens used and a breakdown by model. Needs [Claude Code](https://claude.com/claude-code).

### Codex

Your limits, credits, free resets, streak, records and half a year of daily use. Needs [Codex](https://openai.com/codex/) signed in with ChatGPT.

<p>
  <img src="docs/screenshots/codex.jpg" width="49%" alt="The Codex tab with a limit charted against an even pace" />
  <img src="docs/screenshots/codex-calendar.jpg" width="49%" alt="A calendar of daily token use above a set of records" />
</p>

The pace chart draws a limit across its whole window, against a line for even use. Stay under the line and you have room to spare. Cross it, and Pulse tells you when you'd run out.

## Your data

Nothing connects until you say so. Before Pulse reads anything, it shows you exactly what it will do and what it never will, and you can disconnect at any time in Settings.

<p>
  <img src="docs/screenshots/connect.jpg" width="49%" alt="The Connect Codex sheet" />
  <img src="docs/screenshots/settings.jpg" width="49%" alt="Pulse Settings" />
</p>

- **Your figures stay on your computer.** Pulse has no account, no server and no analytics.
- **Pulse never sees your logins.** Claude data comes from what Claude Code keeps on your computer. Codex data comes through Codex itself, which contacts OpenAI using its own sign-in.
- **Conversations are never kept.** Claude transcripts are read to count tokens, and only the counts are stored.

## Settings

- Choose the shortcut that opens Pulse.
- Choose the stat shown on the tray icon, and the tab Pulse opens to.
- Keep Pulse ready, which opens it instantly for about 250 MB more memory.
- Start at login.
- Keep history for 1 week, 2 weeks or 30 days.

## Build from source

You need [Node.js](https://nodejs.org), [pnpm](https://pnpm.io), [Rust](https://rustup.rs) and the [Tauri prerequisites for Windows](https://v2.tauri.app/start/prerequisites/).

```sh
pnpm install
pnpm tauri dev
```

To build the installer:

```sh
pnpm tauri build
```

Pulse is built with [Tauri](https://tauri.app), Rust and React. It has been tested on Windows 11.

## License

[MIT](LICENSE)
