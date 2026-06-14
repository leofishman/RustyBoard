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

fn is_tauri() -> bool {
    let window = match web_sys::window() {
        Some(w) => w,
        None => return false,
    };
    let tauri_val = match js_sys::Reflect::get(&window, &JsValue::from_str("__TAURI__")) {
        Ok(v) => v,
        Err(_) => return false,
    };
    !tauri_val.is_undefined() && !tauri_val.is_null()
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
    on_run_plugin: Action<(String, String), (), LocalStorage>,
    on_trigger_warning: WriteSignal<Option<(String, String, String)>>,
    #[prop(into)] is_active: Signal<bool>,
    on_delete: Action<String, (), LocalStorage>,
) -> impl IntoView {
    let (revealed, set_revealed) = signal(false);
    let (copied_indicator, set_copied_indicator) = signal(false);
    let (view_raw, set_view_raw) = signal(false);
    let (show_plugins, set_show_plugins) = signal(false);
    let (deleting, set_deleting) = signal(false);

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
            {move || {
                if deleting.get() {
                    view! {
                        <div class="card-deleting-state">
                            <div class="spinner-small"></div>
                            <span>"Deleting item..."</span>
                        </div>
                    }.into_any()
                } else {
                    let on_copy = on_copy.clone();
                    let on_run_plugin = on_run_plugin.clone();
                    let on_trigger_warning = on_trigger_warning.clone();
                    let plugins = plugins.clone();
                    let is_active = is_active.clone();
                    let id = id.clone();
                    let id_mermaid = id_mermaid.clone();
                    let id_plugin = id_plugin.clone();
                    let ct_class = ct_class.clone();
                    let dt_class = dt_class.clone();
                    let ct_tag = ct_tag.clone();
                    let dt_tag = dt_tag.clone();
                    let time_str = time_str.clone();
                    let ct_body = ct_body.clone();
                    let dt_body = dt_body.clone();
                    let dc_body = dc_body.clone();
                    let ct_footer = ct_footer.clone();
                    let ct_plugin = ct_plugin.clone();

                    view! {
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
                            <div class="card-header-right">
                                <span class="timestamp">{time_str}</span>
                                <button class="btn-delete-small" title="Delete from history" on:click={
                                    let on_delete = on_delete.clone();
                                    let delete_id = id.clone();
                                    let set_deleting = set_deleting.clone();
                                    move |_| {
                                        set_deleting.set(true);
                                        on_delete.dispatch(delete_id.clone());
                                    }
                                }>
                                    "🗑️"
                                </button>
                            </div>
                        </div>

                        <div class="card-body">
                            {
                                let ct_body = ct_body.clone();
                                let dt_body = dt_body.clone();
                                let dc_body = dc_body.clone();
                                move || {
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
                                            <div class="image-content">
                                                <img src=dc_body.clone() alt="Clipboard Image" />
                                            </div>
                                        }.into_any()
                                    } else {
                                        match dt_body {
                                            DetectedType::Svg => {
                                                view! {
                                                    <div class="svg-content" inner_html=dc_body.clone() />
                                                }.into_any()
                                            }
                                            DetectedType::Mermaid => {
                                                let container_id = format!("mermaid-{}", id_mermaid);
                                                view! {
                                                    <div class="mermaid-container">
                                                        {
                                                            if view_raw.get() {
                                                                view! { <pre class="raw-text"><code>{dc_body.clone()}</code></pre> }.into_any()
                                                            } else {
                                                                view! { <div id=container_id.clone() class="mermaid" data-definition=dc_body.clone()>{dc_body.clone()}</div> }.into_any()
                                                            }
                                                        }
                                                    </div>
                                                }.into_any()
                                            }
                                            DetectedType::Markdown => {
                                                view! {
                                                    <div class="markdown-container">
                                                        {
                                                            if view_raw.get() {
                                                                view! { <pre class="raw-text"><code>{dc_body.clone()}</code></pre> }.into_any()
                                                            } else {
                                                                let html = render_markdown(&dc_body);
                                                                view! { <div class="markdown-body" inner_html=html /> }.into_any()
                                                            }
                                                        }
                                                    </div>
                                                }.into_any()
                                            }
                                            _ => {
                                                view! {
                                                    <pre class="raw-text"><code>{dc_body.clone()}</code></pre>
                                                }.into_any()
                                            }
                                        }
                                    }
                                }
                            }
                        </div>

                        <div class="card-footer">
                            <div class="card-actions">
                                {
                                    let sensitivity = sensitivity.clone();
                                    move || {
                                        if sensitivity == Sensitivity::Secret || sensitivity == Sensitivity::Credential {
                                            view! {
                                                <button class="btn btn-secondary" on:click=move |_| set_revealed.update(|r| *r = !*r)>
                                                    {move || if revealed.get() { "🙈 Hide" } else { "👁️ Reveal" }}
                                                </button>
                                            }.into_any()
                                        } else {
                                            ().into_any()
                                        }
                                    }
                                }

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

                                {
                                    let on_copy = on_copy.clone();
                                    let copy_id = id.clone();
                                    let set_copied_indicator = set_copied_indicator.clone();
                                    let is_active = is_active.clone();
                                    move || {
                                        if is_active.get() {
                                            ().into_any()
                                        } else {
                                            let on_copy = on_copy.clone();
                                            let copy_id = copy_id.clone();
                                            let set_copied_indicator = set_copied_indicator.clone();
                                            view! {
                                                <button class="btn btn-primary" on:click=move |_| {
                                                    on_copy.dispatch(copy_id.clone());
                                                    set_copied_indicator.set(true);
                                                    set_timeout(move || {
                                                        set_copied_indicator.set(false);
                                                    }, 1500);
                                                }>
                                                    {move || if copied_indicator.get() { "✓ Copied!" } else { "📋 Copy" }}
                                                </button>
                                            }.into_any()
                                        }
                                    }
                                }

                                {
                                    let ct_plugin_clone = ct_plugin.clone();
                                    let on_run = on_run_plugin.clone();
                                    let iid = id_plugin.clone();
                                    let trigger_warning = on_trigger_warning.clone();
                                    let set_show = set_show_plugins.clone();
                                    move || {
                                        let applicable: Vec<PluginDefinition> = plugins.get()
                                            .into_iter()
                                            .filter(|p| p.accepts(plugin_char_count, plugin_word_count) && p.accepts_type(plugin_type_id))
                                            .collect();
                                        if ct_plugin_clone == "text" && !applicable.is_empty() {
                                            let on_run = on_run.clone();
                                            let iid = iid.clone();
                                            let trigger_warning = trigger_warning.clone();
                                            let set_show = set_show.clone();
                                            view! {
                                                <div class="plugin-dropdown">
                                                    <button class="btn btn-tertiary" on:click=move |_| set_show.update(|s| *s = !*s)>
                                                        "🔌 Plugins"
                                                    </button>
                                                    {
                                                        let on_run = on_run.clone();
                                                        let iid = iid.clone();
                                                        let trigger_warning = trigger_warning.clone();
                                                        let set_show = set_show.clone();
                                                        move || {
                                                            if show_plugins.get() {
                                                                let on_run = on_run.clone();
                                                                let iid = iid.clone();
                                                                let trigger_warning = trigger_warning.clone();
                                                                let set_show = set_show.clone();
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
                                                                                let name_for_click = name.clone();
                                                                                let desc = p.description.clone();
                                                                                let on_run = on_run.clone();
                                                                                let iid = iid.clone();
                                                                                let trigger_warning = trigger_warning.clone();
                                                                                let set_show = set_show.clone();
                                                                                view! {
                                                                                    <button class="plugin-item" title=desc on:click=move |_| {
                                                                                        set_show.set(false);
                                                                                        let pid_clone = pid.clone();
                                                                                        let iid_clone = iid.clone();
                                                                                        let name_clone = name_for_click.clone();
                                                                                        let already_accepted = is_plugin_accepted(&pid_clone);

                                                                                        if already_accepted {
                                                                                            on_run.dispatch((pid_clone, iid_clone));
                                                                                        } else {
                                                                                            trigger_warning.set(Some((pid_clone, iid_clone, name_clone)));
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
                    }.into_any()
                }
            }}
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
    let (pending_plugin_run, set_pending_plugin_run) = signal(None::<(String, String, String)>); // (plugin_id, item_id, plugin_name)
    let (dont_show_again, set_dont_show_again) = signal(false);

    // 1. Initial Load of History and Plugins
    Effect::new(move |_| {
        if is_tauri() {
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
        } else {
            // Provide some mock history items when running in the browser so the user can preview the UI!
            set_history.set(vec![
                UIClipboardItem {
                    id: "mock-1".to_string(),
                    display_content: "Welcome to RustyBoard! This is a mock clipboard item for browser preview.".to_string(),
                    content_type: "text".to_string(),
                    sensitivity: Sensitivity::None,
                    timestamp: 1718320000,
                },
                UIClipboardItem {
                    id: "mock-2".to_string(),
                    display_content: "admin@rustyboard.org".to_string(),
                    content_type: "text".to_string(),
                    sensitivity: Sensitivity::Personal,
                    timestamp: 1718318000,
                },
                UIClipboardItem {
                    id: "mock-3".to_string(),
                    display_content: "•••••••• [Secret]".to_string(),
                    content_type: "text".to_string(),
                    sensitivity: Sensitivity::Secret,
                    timestamp: 1718316000,
                }
            ]);
        }
    });

    // 1b. Listen for window-shown events to refresh history
    Effect::new(move |_| {
        if is_tauri() {
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
        }
    });

    // 1c. Listen for history-synced events (periodic background cleanup sync)
    Effect::new(move |_| {
        if is_tauri() {
            let set_history = set_history.clone();
            let closure = Closure::<dyn Fn(JsValue)>::new(move |event_payload: JsValue| {
                if let Ok(payload) = js_sys::Reflect::get(&event_payload, &JsValue::from_str("payload")) {
                    if let Ok(items) = serde_wasm_bindgen::from_value::<Vec<UIClipboardItem>>(payload) {
                        set_history.set(items);
                    }
                }
            });

            let handler = closure.as_ref().unchecked_ref::<js_sys::Function>().clone();
            closure.forget();

            spawn_local(async move {
                listen("history-synced", &handler).await;
            });
        }
    });

    // 2. Initial Load of persist_level configuration
    Effect::new(move |_| {
        if is_tauri() {
            spawn_local(async move {
                let val = invoke("get_persist_level", JsValue::UNDEFINED).await;
                if let Some(s) = val.as_string() {
                    set_persist_level.set(s);
                }
            });
        } else {
            set_persist_level.set("Sensitive".to_string());
        }
    });

    // 3. Listen for Real-Time Clipboard Events
    Effect::new(move |_| {
        if is_tauri() {
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
        }
    });

    // 4. Action to Copy back to OS Clipboard (thread-local since futures are not Send)
    let copy_item = Action::new_local(|id: &String| {
        let id = id.clone();
        async move {
            if is_tauri() {
                let args = serde_wasm_bindgen::to_value(&CopyArgs { id }).unwrap();
                invoke("copy_to_clipboard", args).await;
            }
        }
    });

    // 4c. Action to Delete from History
    let delete_item = Action::new_local({
        let set_history = set_history.clone();
        move |id: &String| {
            let id = id.clone();
            let set_history = set_history.clone();
            async move {
                if is_tauri() {
                    let args = serde_wasm_bindgen::to_value(&CopyArgs { id: id.clone() }).unwrap();
                    let _ = invoke("delete_clipboard_item", args).await;
                }
                set_history.update(|h| {
                    h.retain(|item| item.id != id);
                });
            }
        }
    });

    // 4d. Action to Clear All History
    let clear_all = Action::new_local({
        let set_history = set_history.clone();
        move |_: &()| {
            let set_history = set_history.clone();
            async move {
                if is_tauri() {
                    let _ = invoke("clear_all_history", JsValue::UNDEFINED).await;
                }
                set_history.set(Vec::new());
            }
        }
    });

    let handle_clear_all = move |_| {
        clear_all.dispatch(());
    };

    // 4b. Action to Run Plugin
    let run_plugin = Action::new_local(move |args: &(String, String)| {
        let plugin_id = args.0.clone();
        let item_id = args.1.clone();
        async move {
            set_plugin_error.set(None);
            if is_tauri() {
                let invoke_args = serde_wasm_bindgen::to_value(&RunPluginArgs { plugin_id, item_id }).unwrap();
                if let Err(e) = invoke_catch("run_plugin", invoke_args).await {
                    let msg = e.as_string().unwrap_or_else(|| "Plugin execution failed.".to_string());
                    set_plugin_error.set(Some(msg));
                }
            } else {
                set_plugin_error.set(Some("Plugins are only available in the native desktop app.".to_string()));
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
                if is_tauri() {
                    let args = serde_wasm_bindgen::to_value(&LevelArgs { value }).unwrap();
                    invoke("set_persist_level", args).await;
                }
            });
        }
    };

    let confirm_all_persistence = move |_| {
        set_show_confirm_modal.set(false);
        set_persist_level.set("All".to_string());
        spawn_local(async move {
            if is_tauri() {
                let args = serde_wasm_bindgen::to_value(&LevelArgs { value: "All".to_string() }).unwrap();
                invoke("set_persist_level", args).await;
            }
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
            {move || {
                if !is_tauri() {
                    view! {
                        <div class="browser-warning-banner">
                            "⚠️ Running in Browser Sandbox. Run " <code>"cargo tauri dev"</code> " to launch the native desktop app with clipboard syncing."
                        </div>
                    }.into_any()
                } else {
                    view! { <div style="display: none;"></div> }.into_any()
                }
            }}
            <header class="header">
                <div class="logo-area">
                    <div class="logo-title-row">
                        <svg class="app-logo" viewBox="0 0 24 24" fill="none">
                            <rect x="5" y="4" width="14" height="17" rx="2.5" fill="url(#rustyGradient)" stroke="#5c2108" stroke-width="1" />
                            <rect x="8" y="2.5" width="8" height="3" rx="0.75" fill="url(#metalGradient)" stroke="#334155" stroke-width="0.75" />
                            <rect x="7.5" y="9.5" width="9" height="1.2" rx="0.6" fill="#451a03" />
                            <rect x="7.5" y="13.5" width="9" height="1.2" rx="0.6" fill="#451a03" />
                            <rect x="7.5" y="17.5" width="6.5" height="1.2" rx="0.6" fill="#451a03" />
                            <circle cx="12" cy="4" r="0.6" fill="#f59e0b" stroke="#78350f" stroke-width="0.4" />
                            <defs>
                                <linearGradient id="rustyGradient" x1="0%" y1="0%" x2="100%" y2="100%">
                                    <stop offset="0%" stop-color="#b45309" />
                                    <stop offset="35%" stop-color="#ea580c" />
                                    <stop offset="70%" stop-color="#78350f" />
                                    <stop offset="100%" stop-color="#451a03" />
                                </linearGradient>
                                <linearGradient id="metalGradient" x1="0%" y1="0%" x2="0%" y2="100%">
                                    <stop offset="0%" stop-color="#cbd5e1" />
                                    <stop offset="50%" stop-color="#94a3b8" />
                                    <stop offset="100%" stop-color="#475569" />
                                </linearGradient>
                            </defs>
                        </svg>
                        <h1>"RustyBoard"</h1>
                    </div>
                    {move || {
                        let level = persist_level.get();
                        if level == "All" {
                            view! { <span class="shield-unrestricted">"⚠️ Unrestricted Mode"</span> }.into_any()
                        } else {
                            view! { <span class="shield">"🛡️ Security-First Active"</span> }.into_any()
                        }
                    }}
                </div>
                <div class="settings-area">
                    <label class="setting-label">
                        <select class="settings-select" prop:value=persist_level on:change=handle_level_change>
                            <option value="None">"Paranoid (Strict - No Sensitive Data)"</option>
                            <option value="Sensitive">"Balanced (Credentials with TTL)"</option>
                            <option value="All">"Unrestricted (Persist Secrets)"</option>
                        </select>
                    </label>
                    <button class="btn-clear-all" on:click=handle_clear_all>
                        "🗑️ Clear All"
                    </button>
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
                                children={
                                    let copy_item = copy_item.clone();
                                    let run_plugin = run_plugin.clone();
                                    let set_pending_plugin_run = set_pending_plugin_run.clone();
                                    let delete_item = delete_item.clone();
                                    let history = history.clone();
                                    move |item| {
                                        let item_id = item.id.clone();
                                        let is_active = Signal::derive(move || {
                                            history.get().first().map(|x| x.id.clone()) == Some(item_id.clone())
                                        });
                                        view! {
                                            <ClipboardCard
                                                item=item.clone()
                                                on_copy=copy_item.clone()
                                                plugins=plugins.into()
                                                on_run_plugin=run_plugin.clone()
                                                on_trigger_warning=set_pending_plugin_run.clone()
                                                is_active=is_active
                                                on_delete=delete_item.clone()
                                            />
                                        }
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

            {
                let on_run = run_plugin.clone();
                let set_pending = set_pending_plugin_run.clone();
                let dont_show = dont_show_again.clone();
                move || {
                    if let Some((pid, iid, name)) = pending_plugin_run.get() {
                        let on_run = on_run.clone();
                        let set_pending = set_pending.clone();
                        let dont_show = dont_show.clone();
                        let pid_clone = pid.clone();
                        let iid_clone = iid.clone();
                        view! {
                            <div class="modal-overlay">
                                <div class="modal-card">
                                    <div class="modal-header">
                                        <span class="warning-icon">"⚠️"</span>
                                        <h2>"Security Warning: External Plugin"</h2>
                                    </div>
                                    <div class="modal-body">
                                        <p>"You are about to execute an external command/script on your system via: " <strong>{name}</strong></p>
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
                                            set_pending.set(None);
                                            set_dont_show_again.set(false);
                                        }>
                                            "Cancel"
                                        </button>
                                        <button class="btn btn-danger" on:click=move |_| {
                                            if dont_show.get() {
                                                accept_plugin(&pid_clone);
                                            }
                                            on_run.dispatch((pid_clone.clone(), iid_clone.clone()));
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
