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
- **Premium UI**: Styled with a dark glassmorphism theme, custom tags, and interactive action buttons.

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
To verify security filters and sanitization rules:
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
- [ ] **Phase 6: Secure Local Storage** (SQLite local storage, master key integration with OS keyring, and automatic TTL pruning).
