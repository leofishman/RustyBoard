# RustyBoard 🦀📋

RustyBoard is a high-performance, secure clipboard manager built entirely in Rust. It utilizes **Tauri 2** for native operating system interactions (System Tray, Global Hotkeys, Clipboard access) and **Leptos v0.7** (WebAssembly CSR) for a lightweight, reactive user interface.

> [!WARNING]
> This is a personal learning project and is currently in active development.

---

## 🛡️ Security-First Architecture

RustyBoard is designed around a zero-trust model for clipboard data:

1. **Double Channel (Doble Canal) Strategy**:
   - **`raw_content`**: The original, unaltered content captured from the OS. This remains safely on the backend, is never exposed to the Leptos WebView, and is only written back to the clipboard when you explicitly request a copy.
   - **`display_content`**: A sanitized version of the content used exclusively for UI rendering.
2. **SVG Sanitization**: Any copied vector graphic is parsed and scrubbed on the backend to strip `<script>`, `<iframe>`, `<object>`, `<embed>`, and inline event handlers (like `onload` or `onclick`) to prevent Cross-Site Scripting (XSS) in the WebView.
3. **Entropy & Sensitivity Classification**:
   All incoming text is evaluated for sensitive data:
   - `Secret`: Passwords, credit cards, private keys. They are masked by default, zeroized, and excluded from persistent storage policies.
   - `Credential`: API keys, auth tokens. Masked by default in the UI with a reveal toggle.
   - `Personal`: Emails and phone numbers. Styled with a privacy indicator.
   - `None`: Normal text, links, and images.
4. **Tauri Isolation Pattern**:
   A secure, isolated JavaScript sandboxed iframe is introduced between the main frontend WebView and the Rust core. All IPC payloads are intercepted, verified, and AES-GCM encrypted. This ensures that even if a frontend dependency is compromised, it cannot execute arbitrary backend calls or bypass parameters validation.

---

## ✨ Features

- **Tauri Isolation Sandbox**: Intercepts IPC messages dynamically to validate arguments (such as verifying that a dynamic hotkey string is under 50 characters) before passing them to the system.
- **Event-Driven Clipboard Monitoring**: Uses `clipboard-master` to hook into native OS events (XFixes selection events on Linux X11, `WM_CLIPBOARDUPDATE` on Windows, and NSPasteboard `changeCount` on macOS). This ensures **0% CPU utilization** when the clipboard is idle.
- **Configurable Global Shortcuts**: Features hotkey integration via `tauri-plugin-global-shortcut` to toggle the visibility of the primary window. Custom hotkeys are validated and registered dynamically at runtime.
- **Persistent Settings**: Automatically loads and stores your customized shortcut configuration inside `config.json` in the user's config directory.
- **Feedback Loop Prevention**: Automatically detects and ignores clip changes originating from within the app itself.
- **Image Support**: Captures raw clipboard screenshots, encodes them to PNG, and renders them reactively.
- **Extensible Detector Pipeline**: The frontend classifies data and presents customized cards:
  - **SVG**: Renders inline vector graphics safely.
  - **JSON**: Formats and beautifies code blocks.
  - **URL**: Displays interactive clickable links.
  - **TEXT**: Displays wrapped multi-line text blocks.
- **SQLite Database Persistence**: Stores history locally on disk with configurable levels:
  - **Paranoid**: Persists only non-sensitive clips (default).
  - **Balanced**: Persists credentials (with a 2-hour TTL expiration) and personal details.
  - **Unrestricted**: Persists everything, including secrets, after presenting a warning confirmation modal.
- **Markdown & Mermaid Rendering**: Native compiler for styled Markdown text and live rendering of dynamic Mermaid graphs (flowcharts, state diagrams).
- **Preview/Raw Toggle**: Allows cards with specialized formatting (Markdown, Mermaid, JSON, SVG) to be dynamically toggled between raw source code/markup and rendered graphical views.
- **Dynamic System Tray Clipboard Menu (Diodon-style)**: Lists the 15 most recent clipboard history entries directly in the tray menu (featuring newline cleanup, truncation, media indicators, and secure credential masking). Selecting any item copies it back to the active OS clipboard.
- **Automatic History Refresh**: Listening to Tauri window show events to automatically reload and refresh history when the window is shown or focused.
- **System Tray Background Daemon**: Intercepts close events to hide the app to the system tray, allowing it to run in the background. Left-clicking the icon toggles visibility.
- **Premium UI**: Styled with a dark glassmorphism theme, custom tags, dynamic confirmation dialogs, and interactive action buttons.

---

## 🚀 Quick Start

### Prerequisites
Ensure you have Rust, Cargo, and Tauri dependencies installed on your system. You also need [Trunk](https://trunkrs.dev/) for compiling the WebAssembly frontend.

### Running Development Server
To launch the application in development mode:
```bash
cargo tauri dev
```

### Running Backend Unit Tests
To verify security filters, database rules, and sanitization models:
```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib
```

---

## 📋 Development Roadmap

- [x] **Phase 1: Security & Sanitization Module** (Input validation, SVG scrubbing, and data sensitivity analysis).
- [x] **Phase 2: Backend & IPC Commands** (Clipboard polling, Double Channel storage, feedback loop suppression).
- [x] **Phase 3: Frontend & Extensible Detectors** (Leptos event listening, dynamic card layouts, dark CSS theme).
- [x] **Phase 4: Event-Driven Clipboard & Configurable Shortcuts** (Zero-CPU idle monitor, persistent config file, dynamic window toggle hotkey).
- [x] **Phase 5: Tauri Isolation Pattern** (Secure JS iframe sandbox, IPC interception, dynamic argument validation, and payload encryption).
- [x] **Phase 6: Secure Local Storage** (SQLite local storage, configurable persistence levels, and automatic TTL pruning).
- [x] **Phase 7: Markdown & Mermaid Rendering** (Safe native Markdown compilation, dynamic inline Mermaid vector diagrams).
- [x] **Phase 8: Persistence & System Tray** (System tray daemon mode, window-close interception to tray, and warning modal for advanced persistence).
- [x] **Phase 9: Dynamic Tray History & Preview/Raw Switching** (Diodon-style tray menu items, click-to-copy from tray, card view toggling, and reload on focus).
- [x] **Phase 10: External Plugin System** (Extensible CLI-based plugin system to run external tools on clipboard contents like Fabric AI).
- [x] **Phase 11: Safety Modal & Stdin/Stdout Sync** (One-time safety warning dialog with "do not show again" preference, and writing plugin outputs directly to the system clipboard).
- [ ] **Phase 12: Validate & Refine "Clear All"** (Rigorous cross-platform testing of the asynchronous clear all command and UI button layouts).
- [ ] **Phase 13: Multi-Language Support (i18n)** (Zero-dependency lightweight localization for English, Spanish, and other languages).
- [ ] **Phase 14: OS Malware Mitigation (Auto-Type)** (Keystroke simulation / virtual typing to enter clips directly into active input fields without using the system clipboard).


---

## 🔌 Plugins

RustyBoard supports an extensible plugin system that allows the community to build custom commands using external CLI tools.
Plugins are defined using simple `.json` files placed in the `plugins/` directory within your app's configuration folder (e.g. `~/.config/rustyboard/plugins/`).

RustyBoard ships **with no plugins enabled by default** — we keep the app lightweight and let you opt into extending it. On first launch only a single `uppercase.json` sample is written to your config folder; everything under `src-tauri/plugins/` in this repository is **reference documentation**, not installed automatically. To use one, copy its `.json` (and any script it references) into your config `plugins/` directory yourself.

### How It Works

When a plugin is invoked, RustyBoard takes the *raw text content* of the clipboard item and pipes it directly into the `stdin` of the defined command. The `stdout` of that command is then safely captured, sanitized, and injected back into RustyBoard as a brand new clipboard entry!

#### Input constraints (optional)

A plugin can declare what kind of input it can sensibly handle, and RustyBoard will only offer it for clipboard items that match. This keeps, for example, a single-word dictionary lookup or a short search from showing up when you've copied an entire document, and lets a JSON formatter appear only for JSON:

```json
{
  "id": "ts-grokpedia",
  "name": "Search Grokipedia",
  "command": "bun",
  "args": ["run", "grokpedia.ts"],
  "max_words": 4,
  "max_chars": 60
}
```

| Field | Meaning |
|-------|---------|
| `max_chars` | Hide the plugin when the item has more characters than this. |
| `max_words` | Hide the plugin when the item has more whitespace-separated words than this. |
| `applies_to` | List of detected content types the plugin applies to. Valid values: `text`, `url`, `json`, `svg`, `mermaid`, `markdown`. |

All three fields are optional; omit them and the plugin is offered for any text item. Plugins currently operate on text only — image support is planned but not yet wired into the execution path.

```json
{
  "id": "prettify-json",
  "name": "Prettify JSON",
  "command": "python3",
  "args": ["-m", "json.tool"],
  "applies_to": ["json"]
}
```

### Example: Fabric AI Integration

You can easily integrate external AI workflows, like [Fabric](https://github.com/danielmiessler/fabric), by creating a `fabric-summary.json` file in the plugins folder:

```json
{
  "id": "fabric-summary",
  "name": "Summarize with Fabric AI",
  "description": "Uses Fabric AI to summarize the copied text",
  "command": "fabric",
  "args": ["-p", "summarize"]
}
```

Now, any text you copy can be summarized with a single click from the UI! The community is encouraged to create and share their own custom `.json` plugins.

### 🔒 Safety & Clipboard Synchronization

- **Safety Warning Modal**: To protect against accidental execution of unvetted local binaries, RustyBoard shows a warning modal in English the first time you execute a given plugin. Trust is granted **per plugin** — checking "Trust this plugin and don't warn me again for it" only silences the warning for that specific plugin, so a different (or newly added) plugin will still prompt you. Choices persist in local storage.
- **Execution Timeout**: Each plugin process is given a hard 30-second limit; if it hangs (e.g. waiting on the network) it is terminated and the error is surfaced in the UI.
- **Error Reporting**: If a plugin fails or exits non-zero, its `stderr` is shown in a dismissible toast instead of failing silently.
- **System Clipboard Integration**: The text output of any executed plugin is automatically written back to your OS clipboard, making it instantly available for paste actions anywhere.

### Example Plugins included

We have included some fully commented, **working** examples inside the `src-tauri/plugins/examples` folder to help you get started. They all hit real APIs and need no extra dependencies beyond their runtime:
1. `translator.py`: A Python script (standard library only) that translates the clipboard content via Google's public `gtx` endpoint, auto-detecting the source language. Target language defaults to English and can be overridden, e.g. `"args": ["translator.py", "es"]`.
2. `dictionary.sh`: A Bash script that takes a single word and fetches its definition from the free Dictionary API, formatting it as Markdown using `jq` or `python3` (whichever is available).
3. `grokpedia.ts`: A TypeScript plugin that runs a **real search against [Grokipedia](https://grokipedia.com)** and returns the top results as Markdown with links. Run it with `bun run grokpedia.ts` (or `npx tsx grokpedia.ts` on Node.js ≥ 18).
4. `prettify-json.json`: A script-less plugin (just a config) that pretty-prints copied JSON via `python3 -m json.tool`. It uses `applies_to: ["json"]`, so it only appears for JSON items.

