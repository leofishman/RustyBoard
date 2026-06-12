window.__TAURI_ISOLATION_HOOK__ = (payload) => {
  // Intercept and audit IPC commands
  console.log("[Isolation Hook] Intercepted IPC message:", payload.cmd);

  // Validate command arguments before they reach Rust
  if (payload.cmd === "set_shortcut") {
    const shortcutStr = payload.shortcutStr;
    if (typeof shortcutStr !== "string" || shortcutStr.length > 50) {
      throw new Error("[Isolation Hook] Invalid shortcut format or too long");
    }
  }

  // Return payload to allow encryption and processing by Tauri core
  return payload;
};
