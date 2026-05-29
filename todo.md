# 📋 RustyBoard — Tauri 2 + Leptos Development Todo

Este archivo contiene la planificación paso a paso para construir el gestor de portapapeles `RustyBoard` desde cero usando Tauri 2 y Leptos en la UI.

---

## 🛠️ Fase 1: Inicialización del Proyecto
- [ ] **Configurar Entorno**: Ejecutar/verificar el scaffolding de Tauri 2 con Leptos en el frontend (Rust compilado a WebAssembly).
- [ ] **Validar Dev Server**: Correr `cargo tauri dev` y comprobar que la ventana nativa se abra mostrando el frontend de Leptos correctamente.
- [ ] **Limpiar Boilerplate**: Eliminar elementos por defecto que no usemos en la interfaz de Leptos para comenzar con una UI en blanco.

## 💾 Fase 2: Estructuras y Estado Reactivo (IPC)
- [ ] **Definir Modelo de Datos**: Crear el struct `ClipboardItem` en Rust:
  ```rust
  struct ClipboardItem {
      id: String,
      text: String,
      copied_at: i64, // Unix timestamp
  }
  ```
- [ ] **Estado en Leptos**: Crear un `Signal<Vec<ClipboardItem>>` en el frontend para manejar reactivamente la lista de elementos en la interfaz.
- [ ] **Comandos Tauri (IPC)**: Implementar y registrar un Tauri Command en `main.rs` para permitir agregar elementos manualmente desde la interfaz de Leptos (para pruebas).

## 🔄 Fase 3: Motor de Monitoreo del Portapapeles
- [ ] **Integrar Crate `arboard`**: Añadir la dependencia en `Cargo.toml` para lectura/escritura nativa del clipboard.
- [ ] **Hilo de Monitoreo**: Crear un hilo nativo de Rust (`std::thread` o `tokio::spawn`) que haga *polling* del portapapeles del sistema (ej. cada 500ms).
- [ ] **Eventos en Tiempo Real**: Configurar el hilo para disparar un evento nativo de Tauri (`tauri::Emitter`) cada vez que detecte un texto nuevo.
- [ ] **Escucha en Leptos**: Suscribir el frontend de Leptos al evento de Tauri para añadir automáticamente los nuevos elementos capturados al Signal de la UI.

## 🖥️ Fase 4: Integración del Sistema (Tray & Popup)
- [ ] **System Tray**: Configurar la aplicación para que inicie oculta en segundo plano y muestre un icono en la bandeja del sistema (System Tray).
- [ ] **Shortcuts Globales**: Registrar el atajo de teclado `Ctrl+Shift+V` para invocar/mostrar la ventana desde cualquier parte del sistema operativo.
- [ ] **Posicionamiento Dinámico**: Usar la API de Tauri para obtener las coordenadas del mouse y posicionar la ventana popup directamente sobre la posición del cursor cuando se dispare el shortcut.
