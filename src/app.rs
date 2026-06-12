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

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"])]
    async fn listen(event: &str, handler: &js_sys::Function) -> JsValue;

    #[wasm_bindgen(js_name = renderMermaid)]
    fn render_mermaid(element_id: &str, code: &str);
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
fn ClipboardCard(item: UIClipboardItem, on_copy: Action<String, (), LocalStorage>) -> impl IntoView {
    let (revealed, set_revealed) = signal(false);
    let (copied_indicator, set_copied_indicator) = signal(false);

    let id = item.id.clone();
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
                                let el_id = format!("mermaid-{}", id);
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

                    <button class="btn btn-primary" on:click=handle_copy>
                        {move || if copied_indicator.get() { "✓ Copied!" } else { "📋 Copy" }}
                    </button>
                </div>
            </div>
        </div>
    }
}

#[derive(Serialize)]
struct PersistArgs {
    value: bool,
}

#[component]
pub fn App() -> impl IntoView {
    let (history, set_history) = signal(Vec::<UIClipboardItem>::new());
    let (persist_sensitive, set_persist_sensitive) = signal(false);

    // 1. Initial Load of History
    Effect::new(move |_| {
        spawn_local(async move {
            let val = invoke("get_history", JsValue::UNDEFINED).await;
            if let Ok(items) = serde_wasm_bindgen::from_value::<Vec<UIClipboardItem>>(val) {
                set_history.set(items);
            }
        });
    });

    // 2. Initial Load of persist_sensitive configuration
    Effect::new(move |_| {
        spawn_local(async move {
            let val = invoke("get_persist_sensitive", JsValue::UNDEFINED).await;
            if let Some(b) = val.as_bool() {
                set_persist_sensitive.set(b);
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

    // 5. Toggle persistence configuration
    let handle_toggle = move |_| {
        let next_val = !persist_sensitive.get();
        set_persist_sensitive.set(next_val);
        spawn_local(async move {
            let args = serde_wasm_bindgen::to_value(&PersistArgs { value: next_val }).unwrap();
            invoke("set_persist_sensitive", args).await;
        });
    };

    view! {
        <main class="container">
            <header class="header">
                <div class="logo-area">
                    <h1>"RustyBoard"</h1>
                    <span class="shield">"🛡️ Security-First Active"</span>
                </div>
                <div class="settings-area">
                    <label class="setting-toggle">
                        <input type="checkbox" prop:checked=persist_sensitive on:change=handle_toggle />
                        <span class="toggle-label">"Persist Sensitive Data"</span>
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
                                        <ClipboardCard item=item.clone() on_copy=copy_item />
                                    }
                                }
                            />
                        }.into_any()
                    }
                }}
            </div>
        </main>
    }
}
