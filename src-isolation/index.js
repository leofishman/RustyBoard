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

  if (payload.cmd === "set_persist_sensitive") {
    if (typeof payload.value !== "boolean") {
      throw new Error("[Isolation Hook] Invalid value format for set_persist_sensitive");
    }
  }

  // Return payload to allow encryption and processing by Tauri core
  return payload;
};
