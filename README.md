# Dynooh SiteManager

The lightning-fast, lightweight FTP server manager for Windows. Manage websites, uploads, synchronisation and logins in one place – a lean replacement for the Dreamweaver site manager.

**Download:** [apps.dynooh.com](https://apps.dynooh.com/) (portable `.exe`, no installation)

## Features

- **Sites and groups** – organise all websites in groups, import existing Dreamweaver sites or whole folders
- **Files and FTP** – local and server files side by side, uploads and downloads in a transfer queue
- **Synchronise** – compare a whole site or a single folder, then upload, download or delete differences
- **Built-in editor** – open and edit files directly, several files in tabs
- **Git** – status, commit, pull and push per site
- **Logins and passwords** – stored in the Windows Credential Manager, protected by a master password
- **Encrypted export** – move sites including logins to another computer (Argon2id + AES-256-GCM)
- **Your layout** – light, dark or custom colours, English or German interface
- **Update check** – the app tells you when a newer version is available

## Build from source

Requirements: Windows 10/11, [Node.js](https://nodejs.org/) 20+, [Rust](https://rustup.rs/) (stable), and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).

```sh
npm ci
npx tauri dev                 # development
npx tauri build --no-bundle   # portable exe in src-tauri/target/release/
```

Optional build-time variable `VFSM_MAIL_TOKEN`: shared secret for the password-reset mail endpoint. Without it the app works normally, only “forgot master password” by e-mail is disabled.

The PHP endpoint that sends the reset code lives in [`server/`](server/).

## Code signing policy

Free code signing provided by [SignPath.io](https://about.signpath.io/), certificate by [SignPath Foundation](https://signpath.org/).

- Committers and reviewers: [Visual Fantastix](https://github.com/)
- Approvers: [Visual Fantastix](https://github.com/)

Release binaries are built by GitHub Actions from this repository ([`.github/workflows/release.yml`](.github/workflows/release.yml)) and every signing request is approved manually.

## Privacy

This program does not collect or transmit personal data, with two exceptions that are triggered by the user:

- **Update check:** on start the app downloads `https://apps.dynooh.com/downloads/sitemanager.json` to compare version numbers. No data is sent besides the usual HTTP request.
- **Forgot master password:** the recovery e-mail address and a one-time code are sent to `https://v-f.ch/software/sitemanager/reset-mail.php`, which e-mails the code. Nothing is stored apart from short-lived rate-limit counters.

FTP credentials and passwords never leave your computer except for connecting to your own servers.

## License

[MIT](LICENSE)
