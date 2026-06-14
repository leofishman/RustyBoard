use leptos::task::spawn_local;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use crate::detectors::{classify_text, DetectedType};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    async fn invoke(cmd: &str, args: JsValue) -> JsValue;

    // Same as `invoke`, but surfaces a rejected command (Err) as `Err(JsValue)`.
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], js_name = invoke, catch)]
    async fn invoke_catch(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"])]
    async fn listen(event: &str, handler: &js_sys::Function) -> JsValue;

    #[wasm_bindgen(js_name = renderMermaid)]
    fn render_mermaid(element_id: &str, code: &str);

    #[wasm_bindgen(js_namespace = localStorage, js_name = getItem)]
    fn get_storage_item(key: &str) -> Option<String>;

    #[wasm_bindgen(js_namespace = localStorage, js_name = setItem)]
    fn set_storage_item(key: &str, value: &str);
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Sensitivity {
    None,
    Personal,
    Credential,
    Secret,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct UIClipboardItem {
    id: String,
    display_content: String,
    content_type: String,
    sensitivity: Sensitivity,
    timestamp: u64,
}

#[derive(Serialize)]
struct CopyArgs {
    id: String,
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub max_chars: Option<usize>,
    #[serde(default)]
    pub max_words: Option<usize>,
    #[serde(default)]
    pub applies_to: Option<Vec<String>>,
}

/// Stable lowercase id for a detected content type, used to match a plugin's
/// `applies_to` list against the type of the current clipboard item.
fn detected_type_id(dt: DetectedType) -> &'static str {
    match dt {
        DetectedType::Text => "text",
        DetectedType::Svg => "svg",
        DetectedType::Url => "url",
        DetectedType::Json => "json",
        DetectedType::Mermaid => "mermaid",
        DetectedType::Markdown => "markdown",
    }
}

impl PluginDefinition {
    /// Whether this plugin should be offered for an item with the given text size.
    fn accepts(&self, char_count: usize, word_count: usize) -> bool {
        if let Some(max) = self.max_chars {
            if char_count > max {
                return false;
            }
        }
        if let Some(max) = self.max_words {
            if word_count > max {
                return false;
            }
        }
        true
    }

    /// Whether this plugin applies to the given detected content type.
    /// An unset `applies_to` means it applies to any text item.
    fn accepts_type(&self, type_id: &str) -> bool {
        match &self.applies_to {
            None => true,
            Some(types) => types.iter().any(|t| t.eq_ignore_ascii_case(type_id)),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RunPluginArgs {
    plugin_id: String,
    item_id: String,
}

const ACCEPTED_PLUGINS_KEY: &str = "rustyboard_accepted_plugins";

/// Whether the user has previously trusted this specific plugin (by id).
/// Trust is intentionally per-plugin, so accepting one plugin never silences
/// the warning for a different (possibly malicious) plugin added later.
fn is_plugin_accepted(plugin_id: &str) -> bool {
    get_storage_item(ACCEPTED_PLUGINS_KEY)
        .map(|val| val.split('\n').any(|id| id == plugin_id))
        .unwrap_or(false)
}

/// Persist trust for a single plugin id.
fn accept_plugin(plugin_id: &str) {
    let mut accepted: Vec<String> = get_storage_item(ACCEPTED_PLUGINS_KEY)
        .map(|val| {
            val.split('\n')
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();
    if !accepted.iter().any(|id| id == plugin_id) {
        accepted.push(plugin_id.to_string());
        set_storage_item(ACCEPTED_PLUGINS_KEY, &accepted.join("\n"));
    }
}

fn set_timeout<F: FnOnce() + 'static>(f: F, ms: i32) {
    if let Some(window) = leptos::web_sys::window() {
        let js_func = Closure::once_into_js(f);
        let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
            js_func.unchecked_ref(),
            ms,
        );
    }
}

fn format_timestamp(timestamp: u64) -> String {
    let now = js_sys::Date::now() / 1000.0;
    let diff = now - (timestamp as f64);
    if diff < 0.0 {
        return "Just now".to_string();
    }
    let diff = diff as u64;
    if diff < 5 {
        "Just now".to_string()
    } else if diff < 60 {
        format!("{}s ago", diff)
    } else if diff < 3600 {
        format!("{}m ago", diff / 60)
    } else if diff < 86400 {
        format!("{}h ago", diff / 3600)
    } else {
        let date = js_sys::Date::new(&JsValue::from_f64(timestamp as f64 * 1000.0));
        format!(
            "{:02}/{:02}/{}",
            date.get_date(),
            date.get_month() + 1,
            date.get_full_year()
        )
    }
}

fn render_markdown(md: &str) -> String {
    // Escape raw HTML inside MD first
    let escaped = md.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let parser = pulldown_cmark::Parser::new(&escaped);
    let mut html_output = String::new();
    pulldown_cmark::html::push_html(&mut html_output, parser);
    html_output
}

#[component]
fn ClipboardCard(
    item: UIClipboardItem,
    on_copy: Action<String, (), LocalStorage>,
    plugins: Signal<Vec<PluginDefinition>>,
    on_run_plugin: Action<(String, String), (), LocalStorage>
) -> impl IntoView {
    let (revealed, set_revealed) = signal(false);
    let (copied_indicator, set_copied_indicator) = signal(false);
    let (view_raw, set_view_raw) = signal(false);
    let (show_plugins, set_show_plugins) = signal(false);

    let (show_warning_modal, set_show_warning_modal) = signal(false);
    let (pending_plugin_id, set_pending_plugin_id) = signal(None::<String>);
    let (dont_show_again, set_dont_show_again) = signal(false);

    let id = item.id.clone();
    let id_mermaid = item.id.clone();
    let id_plugin = item.id.clone();
    let content_type = item.content_type.clone();
    let display_content = item.display_content.clone();
    let sensitivity = item.sensitivity;



    let detected_type = if content_type == "text" {
        classify_text(&display_content)
    } else {
        DetectedType::Text
    };

    let is_masked = move || {
        !revealed.get() && (sensitivity == Sensitivity::Secret || sensitivity == Sensitivity::Credential)
    };

    let copy_id = id.clone();
    let handle_copy = move |_| {
        on_copy.dispatch(copy_id.clone());
        set_copied_indicator.set(true);
        set_timeout(move || {
            set_copied_indicator.set(false);
        }, 1500);
    };

    let time_str = format_timestamp(item.timestamp);

    let ct_class = content_type.clone();
    let dt_class = detected_type;
    let ct_tag = content_type.clone();
    let dt_tag = detected_type;

    let ct_body = content_type.clone();
    let dt_body = detected_type;
    let dc_body = display_content.clone();
    let ct_footer = content_type.clone();
    let ct_plugin = content_type.clone();

    // Text size and detected type of this item, used to decide which plugins apply.
    let plugin_char_count = display_content.chars().count();
    let plugin_word_count = display_content.split_whitespace().count();
    let plugin_type_id = if content_type == "text" {
        detected_type_id(detected_type)
    } else {
        "image"
    };

    view! {
        <div class=move || {
            let base = "card";
            let sens_class = match sensitivity {
                Sensitivity::Secret => " sens-secret",
                Sensitivity::Credential => " sens-credential",
                Sensitivity::Personal => " sens-personal",
                Sensitivity::None => "",
            };
            format!("{}{}", base, sens_class)
        }>
            <div class="card-header">
                <div class="tags">
                    <span class=move || {
                        if ct_class == "image" {
                            "tag tag-image"
                        } else {
                            match dt_class {
                                DetectedType::Svg => "tag tag-svg",
                                DetectedType::Url => "tag tag-url",
                                DetectedType::Json => "tag tag-json",
                                DetectedType::Mermaid => "tag tag-mermaid",
                                DetectedType::Markdown => "tag tag-markdown",
                                DetectedType::Text => "tag tag-text",
                            }
                        }
                    }>
                        {
                            if ct_tag == "image" {
                                "IMAGE"
                            } else {
                                match dt_tag {
                                    DetectedType::Svg => "SVG",
                                    DetectedType::Url => "URL",
                                    DetectedType::Json => "JSON",
                                    DetectedType::Mermaid => "MERMAID",
                                    DetectedType::Markdown => "MARKDOWN",
                                    DetectedType::Text => "TEXT",
                                }
                            }
                        }
                    </span>

                    {match sensitivity {
                        Sensitivity::Secret => view! { <span class="badge badge-secret">"SECRET"</span> }.into_any(),
                        Sensitivity::Credential => view! { <span class="badge badge-credential">"CREDENTIAL"</span> }.into_any(),
                        Sensitivity::Personal => view! { <span class="badge badge-personal">"PERSONAL"</span> }.into_any(),
                        Sensitivity::None => ().into_any(),
                    }}
                </div>
                <span class="timestamp">{time_str}</span>
            </div>

            <div class="card-body">
                {move || {
                    if is_masked() {
                        view! {
                            <div class="masked-content">
                                {match sensitivity {
                                    Sensitivity::Secret => "•••••••••••••••• [Encrypted Secret]".to_string(),
                                    Sensitivity::Credential => {
                                        if dc_body.len() > 10 {
                                            format!("{}••••••••{}", &dc_body[0..6], &dc_body[dc_body.len()-4..])
                                        } else {
                                            "••••••••".to_string()
                                        }
                                    },
                                    _ => "".to_string()
                                }}
                            </div>
                        }.into_any()
                    } else if ct_body == "image" {
                        view! {
                            <div class="image-preview">
                                <img src=dc_body.clone() alt="Captured Clip" />
                            </div>
                        }.into_any()
                    } else if view_raw.get() {
                        view! {
                            <pre class="text-content">{dc_body.clone()}</pre>
                        }.into_any()
                    } else {
                        match dt_body {
                            DetectedType::Svg => {
                                view! {
                                    <div class="svg-preview" inner_html=dc_body.clone()></div>
                                }.into_any()
                            }
                            DetectedType::Url => {
                                view! {
                                    <div class="url-preview">
                                        <a href=dc_body.clone() target="_blank" class="url-link">{dc_body.clone()}</a>
                                    </div>
                                }.into_any()
                            }
                            DetectedType::Json => {
                                let formatted = match serde_json::from_str::<serde_json::Value>(&dc_body) {
                                    Ok(val) => serde_json::to_string_pretty(&val).unwrap_or_else(|_| dc_body.clone()),
                                    Err(_) => dc_body.clone(),
                                };
                                view! {
                                    <pre class="json-code"><code>{formatted}</code></pre>
                                }.into_any()
                            }
                            DetectedType::Markdown => {
                                let html = render_markdown(&dc_body);
                                view! {
                                    <div class="markdown-preview" inner_html=html></div>
                                }.into_any()
                            }
                            DetectedType::Mermaid => {
                                let el_id = format!("mermaid-{}", id_mermaid);
                                let code = if dc_body.starts_with("```mermaid") && dc_body.ends_with("```") {
                                    let lines: Vec<&str> = dc_body.lines().collect();
                                    if lines.len() >= 3 {
                                        lines[1..lines.len()-1].join("\n")
                                    } else {
                                        dc_body.clone()
                                    }
                                } else {
                                    dc_body.clone()
                                };

                                let el_id_effect = el_id.clone();
                                Effect::new(move |_| {
                                    let code = code.clone();
                                    let el_id = el_id_effect.clone();
                                    set_timeout(move || {
                                        render_mermaid(&el_id, &code);
                                      }, 50);
                                });

                                view! {
                                    <div id=el_id class="mermaid-container">
                                        <div class="mermaid-loading">"Rendering diagram..."</div>
                                    </div>
                                }.into_any()
                            }
                            DetectedType::Text => {
                                view! {
                                    <pre class="text-content">{dc_body.clone()}</pre>
                                }.into_any()
                            }
                        }
                    }
                }}
            </div>

            <div class="card-footer">
                <div class="card-actions">
                    {if sensitivity == Sensitivity::Secret || sensitivity == Sensitivity::Credential {
                        view! {
                            <button class="btn btn-secondary" on:click=move |_| set_revealed.update(|r| *r = !*r)>
                                {move || if revealed.get() { "🙈 Hide" } else { "👁️ Reveal" }}
                            </button>
                        }.into_any()
                    } else {
                        ().into_any()
                    }}

                    {move || {
                        let is_switchable = ct_footer == "text" && matches!(dt_class, DetectedType::Markdown | DetectedType::Mermaid | DetectedType::Json | DetectedType::Svg);
                        if is_switchable {
                            view! {
                                <button class="btn btn-secondary" on:click=move |_| set_view_raw.update(|r| *r = !*r)>
                                    {move || if view_raw.get() { "👁️ Preview" } else { "📝 Raw" }}
                                </button>
                            }.into_any()
                        } else {
                            ().into_any()
                        }
                    }}

                    <button class="btn btn-primary" on:click=handle_copy>
                        {move || if copied_indicator.get() { "✓ Copied!" } else { "📋 Copy" }}
                    </button>

                    {
                        let ct_plugin_clone = ct_plugin.clone();
                        let on_run = on_run_plugin.clone();
                        let iid = id_plugin.clone();
                        let set_pending = set_pending_plugin_id.clone();
                        let set_warning = set_show_warning_modal.clone();
                        let set_show = set_show_plugins.clone();
                        move || {
                            let applicable: Vec<PluginDefinition> = plugins.get()
                                .into_iter()
                                .filter(|p| p.accepts(plugin_char_count, plugin_word_count) && p.accepts_type(plugin_type_id))
                                .collect();
                            if ct_plugin_clone == "text" && !applicable.is_empty() {
                                let on_run = on_run.clone();
                                let iid = iid.clone();
                                let set_pending = set_pending.clone();
                                let set_warning = set_warning.clone();
                                let set_show = set_show.clone();
                                view! {
                                    <div class="plugin-dropdown">
                                        <button class="btn btn-tertiary" on:click=move |_| set_show.update(|s| *s = !*s)>
                                            "🔌 Plugins"
                                        </button>
                                        {
                                            let on_run = on_run.clone();
                                            let iid = iid.clone();
                                            let set_pending = set_pending.clone();
                                            let set_warning = set_warning.clone();
                                            let set_show = set_show.clone();
                                            move || {
                                                if show_plugins.get() {
                                                    let on_run = on_run.clone();
                                                    let iid = iid.clone();
                                                    let set_pending = set_pending.clone();
                                                    let set_warning = set_warning.clone();
                                                    let set_show = set_show.clone();
                                                    // Lifted out of view! because the turbofish angle brackets
                                                    // would otherwise be parsed as tags by the macro.
                                                    let each_plugins = move || -> Vec<PluginDefinition> {
                                                        plugins.get().into_iter()
                                                            .filter(|p| p.accepts(plugin_char_count, plugin_word_count) && p.accepts_type(plugin_type_id))
                                                            .collect()
                                                    };
                                                    view! {
                                                        <div class="plugin-menu">
                                                            <For
                                                                each=each_plugins
                                                                key=|p| p.id.clone()
                                                                children=move |p| {
                                                                    let pid = p.id.clone();
                                                                    let name = p.name.clone();
                                                                    let desc = p.description.clone();
                                                                    let on_run = on_run.clone();
                                                                    let iid = iid.clone();
                                                                    let set_pending = set_pending.clone();
                                                                    let set_warning = set_warning.clone();
                                                                    let set_show = set_show.clone();
                                                                    view! {
                                                                        <button class="plugin-item" title=desc on:click=move |_| {
                                                                            set_show.set(false);
                                                                            let pid_clone = pid.clone();
                                                                            let iid_clone = iid.clone();
                                                                            let already_accepted = is_plugin_accepted(&pid_clone);

                                                                            if already_accepted {
                                                                                on_run.dispatch((pid_clone, iid_clone));
                                                                            } else {
                                                                                set_pending.set(Some(pid_clone));
                                                                                set_warning.set(true);
                                                                            }
                                                                        }>
                                                                            {name}
                                                                        </button>
                                                                    }
                                                                }
                                                            />
                                                        </div>
                                                    }.into_any()
                                                } else {
                                                    ().into_any()
                                                }
                                            }
                                        }
                                    </div>
                                }.into_any()
                            } else {
                                ().into_any()
                            }
                        }
                    }
                </div>
            </div>

            {
                let on_run = on_run_plugin.clone();
                let iid = id_plugin.clone();
                let set_pending = set_pending_plugin_id.clone();
                let set_warning = set_show_warning_modal.clone();
                let dont_show = dont_show_again.clone();
                move || {
                    if show_warning_modal.get() {
                        let on_run = on_run.clone();
                        let iid = iid.clone();
                        let set_pending = set_pending.clone();
                        let set_warning = set_warning.clone();
                        let dont_show = dont_show.clone();
                        view! {
                            <div class="modal-overlay">
                                <div class="modal-card">
                                    <div class="modal-header">
                                        <span class="warning-icon">"⚠️"</span>
                                        <h2>"Security Warning: External Plugin"</h2>
                                    </div>
                                    <div class="modal-body">
                                        <p>"You are about to execute an external command/script on your system."</p>
                                        <div class="modal-alert">
                                            "Plugins run with your user privileges and can access files, network resources, and execute system commands. "
                                            "Ensure that you trust the plugin configuration and script before executing it."
                                        </div>
                                        <label class="modal-checkbox-label">
                                            <input
                                                type="checkbox"
                                                prop:checked=dont_show
                                                on:change=move |ev| set_dont_show_again.set(event_target_checked(&ev))
                                            />
                                            " Trust this plugin and don't warn me again for it"
                                        </label>
                                    </div>
                                    <div class="modal-footer">
                                        <button class="btn btn-secondary" on:click=move |_| {
                                            set_warning.set(false);
                                            set_pending.set(None);
                                            set_dont_show_again.set(false);
                                        }>
                                            "Cancel"
                                        </button>
                                        <button class="btn btn-danger" on:click=move |_| {
                                            if let Some(pid) = pending_plugin_id.get() {
                                                if dont_show.get() {
                                                    accept_plugin(&pid);
                                                }
                                                on_run.dispatch((pid, iid.clone()));
                                            }
                                            set_warning.set(false);
                                            set_pending.set(None);
                                            set_dont_show_again.set(false);
                                        }>
                                            "Run Plugin"
                                        </button>
                                    </div>
                                </div>
                            </div>
                        }.into_any()
                    } else {
                        ().into_any()
                    }
                }
            }
        </div>
    }
}

#[derive(Serialize)]
struct LevelArgs {
    value: String,
}

#[component]
pub fn App() -> impl IntoView {
    let (history, set_history) = signal(Vec::<UIClipboardItem>::new());
    let (persist_level, set_persist_level) = signal("None".to_string());
    let (show_confirm_modal, set_show_confirm_modal) = signal(false);
    let (plugins, set_plugins) = signal(Vec::<PluginDefinition>::new());
    let (plugin_error, set_plugin_error) = signal(None::<String>);

    // 1. Initial Load of History and Plugins
    Effect::new(move |_| {
        spawn_local(async move {
            let val = invoke("get_history", JsValue::UNDEFINED).await;
            if let Ok(items) = serde_wasm_bindgen::from_value::<Vec<UIClipboardItem>>(val) {
                set_history.set(items);
            }

            let val = invoke("get_plugins", JsValue::UNDEFINED).await;
            if let Ok(items) = serde_wasm_bindgen::from_value::<Vec<PluginDefinition>>(val) {
                set_plugins.set(items);
            }
        });
    });

    // 1b. Listen for window-shown events to refresh history
    Effect::new(move |_| {
        let closure = Closure::<dyn Fn(JsValue)>::new(move |_| {
            spawn_local(async move {
                let val = invoke("get_history", JsValue::UNDEFINED).await;
                if let Ok(items) = serde_wasm_bindgen::from_value::<Vec<UIClipboardItem>>(val) {
                    set_history.set(items);
                }
            });
        });

        let handler = closure.as_ref().unchecked_ref::<js_sys::Function>().clone();
        closure.forget();

        spawn_local(async move {
            listen("window-shown", &handler).await;
        });
    });

    // 2. Initial Load of persist_level configuration
    Effect::new(move |_| {
        spawn_local(async move {
            let val = invoke("get_persist_level", JsValue::UNDEFINED).await;
            if let Some(s) = val.as_string() {
                set_persist_level.set(s);
            }
        });
    });

    // 3. Listen for Real-Time Clipboard Events
    Effect::new(move |_| {
        let set_history = set_history.clone();
        let closure = Closure::<dyn Fn(JsValue)>::new(move |event_payload: JsValue| {
            if let Ok(payload) = js_sys::Reflect::get(&event_payload, &JsValue::from_str("payload")) {
                if let Ok(item) = serde_wasm_bindgen::from_value::<UIClipboardItem>(payload) {
                    set_history.update(|h| {
                        h.insert(0, item);
                        if h.len() > 100 {
                            h.pop();
                        }
                    });
                }
            }
        });

        let handler = closure.as_ref().unchecked_ref::<js_sys::Function>().clone();
        closure.forget();

        spawn_local(async move {
            listen("clipboard-changed", &handler).await;
        });
    });

    // 4. Action to Copy back to OS Clipboard (thread-local since futures are not Send)
    let copy_item = Action::new_local(|id: &String| {
        let id = id.clone();
        async move {
            let args = serde_wasm_bindgen::to_value(&CopyArgs { id }).unwrap();
            invoke("copy_to_clipboard", args).await;
        }
    });

    // 4b. Action to Run Plugin
    let run_plugin = Action::new_local(move |args: &(String, String)| {
        let plugin_id = args.0.clone();
        let item_id = args.1.clone();
        async move {
            set_plugin_error.set(None);
            let invoke_args = serde_wasm_bindgen::to_value(&RunPluginArgs { plugin_id, item_id }).unwrap();
            if let Err(e) = invoke_catch("run_plugin", invoke_args).await {
                let msg = e.as_string().unwrap_or_else(|| "Plugin execution failed.".to_string());
                set_plugin_error.set(Some(msg));
            }
        }
    });

    // 5. Select dropdown change handler
    let handle_level_change = move |ev: leptos::ev::Event| {
        let value = event_target_value(&ev);
        if value == "All" {
            // Show warnings/risk confirmation dialog first!
            set_show_confirm_modal.set(true);
        } else {
            set_persist_level.set(value.clone());
            spawn_local(async move {
                let args = serde_wasm_bindgen::to_value(&LevelArgs { value }).unwrap();
                invoke("set_persist_level", args).await;
            });
        }
    };

    let confirm_all_persistence = move |_| {
        set_show_confirm_modal.set(false);
        set_persist_level.set("All".to_string());
        spawn_local(async move {
            let args = serde_wasm_bindgen::to_value(&LevelArgs { value: "All".to_string() }).unwrap();
            invoke("set_persist_level", args).await;
        });
    };

    let cancel_all_persistence = move |_| {
        set_show_confirm_modal.set(false);
        // Force Leptos to reset the select elements' selected attribute back to actual value
        let current = persist_level.get();
        set_persist_level.set(String::new());
        set_persist_level.set(current);
    };

    view! {
        <main class="container">
            <header class="header">
                <div class="logo-area">
                    <h1>"RustyBoard"</h1>
                    <span class="shield">"🛡️ Security-First Active"</span>
                </div>
                <div class="settings-area">
                    <label class="setting-label">
                        <select class="settings-select" prop:value=persist_level on:change=handle_level_change>
                            <option value="None">"Paranoid (Strict - No Sensitive Data)"</option>
                            <option value="Sensitive">"Balanced (Credentials with TTL)"</option>
                            <option value="All">"Unrestricted (Persist Secrets)"</option>
                        </select>
                    </label>
                    <div class="stats">
                        <span>"Active Clips: " {move || history.get().len()}</span>
                    </div>
                </div>
            </header>

            <div class="history-list">
                {move || {
                    let items = history.get();
                    if items.is_empty() {
                        view! {
                            <div class="empty-state">
                                <span class="empty-icon">"📋"</span>
                                <p>"Clipboard history is empty. Copy some text or images!"</p>
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <For
                                each=move || history.get()
                                key=|item| item.id.clone()
                                children=move |item| {
                                    view! {
                                        <ClipboardCard item=item.clone() on_copy=copy_item plugins=plugins.into() on_run_plugin=run_plugin />
                                    }
                                }
                            />
                        }.into_any()
                    }
                }}
            </div>

            {move || {
                if let Some(err) = plugin_error.get() {
                    view! {
                        <div class="error-toast">
                            <span class="error-toast-icon">"⛔"</span>
                            <span class="error-toast-msg">{err}</span>
                            <button
                                class="error-toast-close"
                                on:click=move |_| set_plugin_error.set(None)
                            >
                                "✕"
                            </button>
                        </div>
                    }.into_any()
                } else {
                    ().into_any()
                }
            }}

            {move || {
                if show_confirm_modal.get() {
                    view! {
                        <div class="modal-overlay">
                            <div class="modal-card">
                                <div class="modal-header">
                                    <span class="warning-icon">"⚠️"</span>
                                    <h2>"High Security Warning"</h2>
                                </div>
                                <div class="modal-body">
                                    <p>"You are about to enable persistence for Secret items."</p>
                                    <div class="modal-alert">
                                        "This includes plain-text passwords, private keys, credit cards, and other tokens. "
                                        "If your system is compromised or if someone gains physical access to your unencrypted disk, they will be able to read these secrets."
                                    </div>
                                    <p>"Are you absolutely sure you want to proceed?"</p>
                                </div>
                                <div class="modal-footer">
                                    <button class="btn btn-secondary" on:click=cancel_all_persistence>
                                        "Cancel"
                                    </button>
                                    <button class="btn btn-danger" on:click=confirm_all_persistence>
                                        "Yes, Persist Secrets"
                                    </button>
                                </div>
                            </div>
                        </div>
                    }.into_any()
                } else {
                    ().into_any()
                }
            }}
        </main>
    }
}
