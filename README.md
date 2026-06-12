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

---

## 🔌 Plugins

RustyBoard supports an extensible plugin system that allows the community to build custom commands using external CLI tools.
Plugins are defined using simple `.json` files placed in the `plugins/` directory within your app's configuration folder (e.g. `~/.config/rustyboard/plugins/`).

### How It Works

When a plugin is invoked, RustyBoard takes the *raw text content* of the clipboard item and pipes it directly into the `stdin` of the defined command. The `stdout` of that command is then safely captured, sanitized, and injected back into RustyBoard as a brand new clipboard entry!

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

### Example Plugins included

We have included some fully commented examples inside the `src-tauri/plugins/examples` folder to help you get started:
1. `translator.py`: A python script that uses `googletrans` to translate the clipboard content to English.
2. `dictionary.sh`: A bash script that takes a single word and fetches its definition from a free dictionary API, returning Markdown format!
3. `grokpedia.ts`: A TypeScript plugin that simulates an API lookup and outputs styled Markdown. It can be run using `bun run grokpedia.ts` or `npx tsx grokpedia.ts`.

