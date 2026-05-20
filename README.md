# RustyBoard
Another Clipboard manager

# 📋 Development Roadmap

## Phase 1: Core Data & Minimal Frontend 🗼
- [ ] Task 1.1: Design the `ClipboardItem` struct in Rust (support text, formats, and timestamps).
- [ ] Task 1.2: Build the Leptos UI to render a scannable list of historical items.
- [ ] Task 1.3: Implement reactive state management (Signals) for manual item addition.

## Phase 2: Background OS Monitoring (Polling) ⏳
- [ ] Task 2.1: Integrate `arboard` crate for cross-platform clipboard access.
- [ ] Task 2.2: Spawn a native Tauri thread to poll the OS clipboard every 500ms.
- [ ] Task 2.3: Implement a listener to trigger Tauri IPC events when changes occur.

## Phase 3: Window Behavior & Pop-up Mechanics 🪟
- [ ] Task 3.1: Configure Tauri to launch headless (System Tray / App Indicator only).
- [ ] Task 3.2: Register global shortcuts (`Ctrl + Shift + V` for standard, custom for private).
- [ ] Task 3.3: Calculate mouse cursor coordinates to position the pop-up window dynamically.

## Phase 4: Storage & Advanced Privacy Layers 💾 🔐
- [ ] Task 4.1: Setup an embedded SQLite database using a lightweight manager (e.g., `rusqlite`).
- [ ] Task 4.2: Implement **AES-256-GCM** encryption for items marked with high-privacy tags.
- [ ] Task 4.3: Create an automated Time-to-Live (TTL) janitor thread to scrub expired private data.
- [ ] Task 4.4: Add active window detection (X11/Wayland) to blacklist clipboard sniffing in target apps.
