# 📋 RustyBoard — Tauri 2 + Leptos Development Todo

Este archivo contiene la planificación paso a paso para construir el gestor de portapapeles `RustyBoard` desde cero usando Tauri 2 y Leptos en la UI.

---

## 🛠️ Fase 1: Inicialización del Proyecto
- [x] **Configurar Entorno**: Ejecutar/verificar el scaffolding de Tauri 2 con Leptos en el frontend (Rust compilado a WebAssembly).
- [x] **Validar Dev Server**: Correr `cargo tauri dev` y comprobar que la ventana nativa se abra mostrando el frontend de Leptos correctamente.
- [x] **Limpiar Boilerplate**: Eliminar elementos por defecto que no usemos en la interfaz de Leptos para comenzar con una UI en blanco.

## 💾 Fase 2: Estructuras y Estado Reactivo (IPC)
- [x] **Definir Modelo de Datos**: Crear el struct `ClipboardItem` en Rust:
  ```rust
  struct ClipboardItem {
      id: String,
      text: String,
      copied_at: i64, // Unix timestamp
  }
  ```
- [x] **Estado en Leptos**: Crear un `Signal<Vec<ClipboardItem>>` en el frontend para manejar reactivamente la lista de elementos en la interfaz.
- [x] **Comandos Tauri (IPC)**: Implementar y registrar un Tauri Command en `main.rs` para permitir agregar elementos manualmente desde la interfaz de Leptos (para pruebas).

## 🔄 Fase 3: Motor de Monitoreo del Portapapeles
- [x] **Integrar Crate `arboard`**: Añadir la dependencia en `Cargo.toml` para lectura/escritura nativa del clipboard.
- [x] **Hilo de Monitoreo**: Crear un hilo nativo de Rust (`std::thread` o `tokio::spawn`) que haga *polling* del portapapeles del sistema (ej. cada 500ms).
- [x] **Eventos en Tiempo Real**: Configurar el hilo para disparar un evento nativo de Tauri (`tauri::Emitter`) cada vez que detecte un texto nuevo.
- [x] **Escucha en Leptos**: Suscribir el frontend de Leptos al evento de Tauri para añadir automáticamente los nuevos elementos capturados al Signal de la UI.

## 🖥️ Fase 4: Integración del Sistema (Tray & Popup)
- [ ] **System Tray**: Configurar la aplicación para que inicie oculta en segundo plano y muestre un icono en la bandeja del sistema (System Tray).
- [ ] **Shortcuts Globales**: Registrar el atajo de teclado `Ctrl+Shift+V` para invocar/mostrar la ventana desde cualquier parte del sistema operativo.
- [ ] **Posicionamiento Dinámico**: Usar la API de Tauri para obtener las coordenadas del mouse y posicionar la ventana popup directamente sobre la posición del cursor cuando se dispare el shortcut.

## 🔌 Fase 5: Sistema de Plugins & Seguridad
- [x] **Ejecución vía CLI**: Implementar el cargador y ejecutor de plugins externos (`src-tauri/src/plugins.rs`).
- [x] **Pase seguro por Stdin**: Comunicar contenido sensible al proceso vía entrada estándar (`stdin`) previniendo inyección de comandos.
- [x] **Sincronización al Portapapeles**: Copiar el resultado de la salida del plugin de forma automática al portapapeles del sistema operativo (`copy_item_by_id`).
- [x] **Cartel de Advertencia (Warning Modal)**: Mostrar una advertencia de seguridad en inglés antes de ejecutar un plugin por primera vez, permitiendo recordar la decisión.
- [x] **Fix mermaid render**: Detección tolerante a fences ```mermaid, llamada explícita a `render_mermaid`, y bundle local (sin CDN).
- [ ] **Zoom y tamaño de diagramas Mermaid**: Los diagramas se renderizan muy chicos. Agregar zoom (rueda del mouse / botones +/−, o pan-zoom) en el preview de Mermaid (y quizá SVG), y remover/relajar el límite de ancho del container (`.container { max-width: 680px }` en `styles.css`) para que los diagramas grandes tengan espacio.
- [ ] **Mermaid embebido en texto/markdown**: Hoy solo se detecta cuando el item *entero* es un diagrama (empieza con `flowchart`/`graph`/```` ```mermaid ````). Detectar y renderizar bloques ```` ```mermaid ```` *dentro* de un texto o markdown más grande (puede haber varios), mostrando prosa + diagramas intercalados en la misma card. Probable implementación: parsear los fenced blocks en `render_markdown` y reemplazar cada bloque mermaid por su contenedor renderizado en vez de tratarlos como código.
- [ ] **Soporte Multi-Idioma (i18n)**: Investigar e implementar soporte multi-idioma para la interfaz (traducción de advertencias, menús y botones).
- [ ] **Mitigación de Malware del SO (Auto-Type)**: Implementar simulación de pulsaciones de teclado (virtual typing) para pegar texto directamente en la ventana activa sin escribirlo en el portapapeles.
- [ ] **Cambiar titulo o nombre de app**: Ahora aparece Tauri-app en el tray y en el panel.

## 🧹 Fase 6: Depuración y Validación Final (Próxima Sesión)
- [ ] **Validar y Refinar Borrado e Historial ("Clear All")**: Resolver los problemas de bloqueo de hilos/interfaz (thread locking) al reconstruir el menú del System Tray durante las eliminaciones, y refinar la visualización de los botones de borrado.

## Fase 7: 
- [ ] **Abstraccion de base de datos**: Soporte de diferentes backends (PostgreSQL, MySQL, mongo, supabase, etc.).
- [ ] **Sync History (One way and Two way)**: Con diferentes dispotivos. Consumir directo de la db (sqlite o la que sea). Mecanismo de autenticacion.
- [ ] **Soportar listas**: Guardar historial como listas para poder gestionar elementos de forma agrupada. No solo una lista de historial y poder compartirlas.
- [ ] **Emular pintado y pegado**: Con boton del medio estilo Linux sin dejar rastro en clipboard.
- [ ] **Heartbeat**: 60 segundos me parece muy frecuente, podria ser cada 15 minutos o cada vez que el usuario haga uso del portapapeles. 
- [ ] **Soporte de atajos complejos**: Actalmente el atajo debe ser Ctrl+Shift+Tecla, me gustaria que se pudieran configurar atajos mas complejos como Super+v, con funcionalidades especificas, por ejemplo la de emular pintado y pegado estilo Linux sin dejar rastro en clipboard, o copiar elementos sensibles mas alla del estado en que funcione la app, un copiado sensible no persiste y se trata como secret aunque el sistema no logre clasificarlo como tal.

## 🔒 Fase 8: Auditoría de seguridad y calidad

Hallazgos de la revisión en profundidad del código (junio 2026). Severidad: 🔴 alto · 🟠 medio · 🟡 bajo.

### Ya resueltos en esta tanda
- [x] **A1 — Semántica de persistencia incoherente**: el modo "Balanced (Credentials with TTL)" no guardaba credentials. Ahora `save_item` en `Sensitive` persiste todo salvo Secret, y `run_cleanup(level)` aplica el TTL de 2h en todos los modos menos `All`.
- [x] **M2 — Mermaid desde CDN remoto**: bundleado local (`vendor/mermaid.min.js`), `securityLevel: 'strict'`, y se agregó la llamada faltante a `render_mermaid` (antes nunca se dibujaba).
- [x] **M3 — Links `javascript:` en markdown**: `render_markdown` neutraliza esquemas peligrosos (`javascript:`/`vbscript:`/`data:`) reescribiendo el href a `#`.
- [x] **M4 — SQLite frágil**: helper `open_conn` con `busy_timeout` (5s) + `journal_mode=WAL`, para evitar pérdida silenciosa de escrituras bajo concurrencia.
- [x] **Reactividad de la lista**: el `<For>` estaba dentro de un bloque reactivo que lo recreaba en cada cambio (rompía borrado múltiple y la actualización al copiar). Se creó una sola vez + `<Show>` para el estado vacío.

### Pendientes
- [ ] 🟠 **M1 — CSP nula**: `tauri.conf.json` tiene `"csp": null`. Combinado con `inner_html` de SVG/markdown y el saneador de SVG por regex (evadible), amplía la superficie XSS. Definir una CSP estricta (cuidado: debe permitir el script local de mermaid y los estilos; testear a fondo que no rompa el render). Opcional: reemplazar el saneador regex de SVG por uno real (ej. `ammonia`).
- [ ] 🟠 **M5 — Secrets en texto plano en disco**: en modo `All` (y datos sensibles en general) no hay cifrado. El roadmap original prometía AES-256-GCM para items privados. Evaluar cifrar el contenido sensible en SQLite (clave derivada / keyring del SO).
- [ ] 🟡 **B2 — Warnings de clippy**: 2 triviales en `src-tauri` (`cargo clippy --fix` los arregla).
- [ ] 🟡 **B3 — Refactor de `ClipboardCard`**: componente enorme y muy anidado (sobre todo el dropdown de plugins), con muchísimos `.clone()` de signals. Descomponer en subcomponentes. Además quedó una señal `deleting` muerta tras el borrado optimista.
- [ ] 🟡 **B4 — Dos rutas de cleanup**: `database::run_cleanup` (SQL) y el `retain` en memoria del loop periódico (`lib.rs`) tienen reglas distintas y pueden divergir. Unificar la lógica en un solo lugar.
- [ ] 🟡 **B5 — `tokio` con `features = ["full"]`**: infla el binario; recortar a las features realmente usadas (`time`, `rt`, etc.).

> Nota: **B1** (renombrar la app de "tauri-app" → RustyBoard en `tauri.conf.json`/`Cargo.toml`) ya está arriba como "Cambiar titulo o nombre de app".
