mod api;
mod bot;
#[allow(dead_code)]
mod ws;
mod xai;

use bot::BotLogEntry;
use dioxus::prelude::*;
use tokio::sync::mpsc;

const FAVICON: Asset = asset!("/assets/favicon.ico");
const MAIN_CSS: Asset = asset!("/assets/main.css");
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");

#[derive(Clone, PartialEq)]
struct ManifoldKey(String);

#[derive(Clone, PartialEq)]
struct XaiKey(String);

#[derive(Clone, PartialEq)]
enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
}

fn main() {
    dotenvy::dotenv().ok();
    dioxus::LaunchBuilder::new()
        .with_cfg(desktop! {
            dioxus::desktop::Config::new().with_menu(None)
        })
        .launch(App);
}

#[component]
fn App() -> Element {
    let env_manifold = std::env::var("MANIFOLD_API_KEY").unwrap_or_default();
    let env_xai = std::env::var("XAI_API_KEY").unwrap_or_default();

    let api_key = use_signal(|| ManifoldKey(env_manifold.clone()));
    let xai_key = use_signal(|| XaiKey(env_xai.clone()));
    let mut user_info = use_signal(|| None::<api::User>);
    let connection_status = use_signal(|| ConnectionStatus::Disconnected);
    let log_entries = use_signal(Vec::<BotLogEntry>::new);
    let ws_events = use_signal(Vec::<String>::new);

    use_context_provider(|| api_key);
    use_context_provider(|| xai_key);
    use_context_provider(|| user_info);
    use_context_provider(|| connection_status);
    use_context_provider(|| log_entries);
    use_context_provider(|| ws_events);

    // Auto-validate if keys came from .env
    let mut auto_started = use_signal(|| false);
    if !auto_started() && !env_manifold.is_empty() && !env_xai.is_empty() {
        auto_started.set(true);
        let mkey = env_manifold.clone();
        spawn(async move {
            let client = api::ManifoldClient::new(mkey);
            if let Ok(user) = client.get_me().await {
                user_info.set(Some(user));
            }
        });
    }

    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }

        div { class: "max-w-4xl mx-auto",
            h1 { class: "text-3xl font-bold mb-6", "Manifold Domination" }

            if user_info.read().is_some() {
                BotDashboard {}
            } else {
                ApiKeyInput {}
            }
        }
    }
}

#[component]
fn ApiKeyInput() -> Element {
    let mut api_key = use_context::<Signal<ManifoldKey>>();
    let mut xai_key = use_context::<Signal<XaiKey>>();
    let mut user_info = use_context::<Signal<Option<api::User>>>();
    let mut manifold_input = use_signal(String::new);
    let mut xai_input = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut loading = use_signal(|| false);

    let mut do_submit = move || {
        let mkey = manifold_input.read().trim().to_string();
        let xkey = xai_input.read().trim().to_string();
        if mkey.is_empty() {
            error.set(Some("Please enter a Manifold API key".to_string()));
            return;
        }
        if xkey.is_empty() {
            error.set(Some("Please enter an xAI API key".to_string()));
            return;
        }
        loading.set(true);
        error.set(None);
        spawn(async move {
            let client = api::ManifoldClient::new(mkey.clone());
            match client.get_me().await {
                Ok(user) => {
                    api_key.set(ManifoldKey(mkey));
                    xai_key.set(XaiKey(xkey));
                    user_info.set(Some(user));
                }
                Err(e) => {
                    error.set(Some(format!("Invalid Manifold API key: {e}")));
                }
            }
            loading.set(false);
        });
    };

    rsx! {
        div { class: "bg-gray-800 rounded-lg p-6 space-y-4",
            h2 { class: "text-xl font-semibold mb-2", "Connect API Keys" }

            div {
                label { class: "block text-sm text-gray-400 mb-1", "Manifold API Key" }
                p { class: "text-gray-500 text-xs mb-2",
                    "Get from "
                    a {
                        href: "https://manifold.markets/profile",
                        class: "text-blue-400 underline",
                        "manifold.markets/profile"
                    }
                }
                input {
                    class: "w-full bg-gray-700 text-white px-4 py-2 rounded border border-gray-600 focus:border-blue-500 focus:outline-none",
                    r#type: "password",
                    placeholder: "Manifold API key...",
                    value: "{manifold_input}",
                    oninput: move |e| manifold_input.set(e.value()),
                }
            }

            div {
                label { class: "block text-sm text-gray-400 mb-1", "xAI API Key" }
                p { class: "text-gray-500 text-xs mb-2",
                    "Get from "
                    a {
                        href: "https://console.x.ai",
                        class: "text-blue-400 underline",
                        "console.x.ai"
                    }
                }
                input {
                    class: "w-full bg-gray-700 text-white px-4 py-2 rounded border border-gray-600 focus:border-blue-500 focus:outline-none",
                    r#type: "password",
                    placeholder: "xAI API key...",
                    value: "{xai_input}",
                    oninput: move |e| xai_input.set(e.value()),
                    onkeydown: move |e: Event<KeyboardData>| {
                        if e.key() == Key::Enter {
                            do_submit();
                        }
                    },
                }
            }

            button {
                class: "w-full bg-blue-600 hover:bg-blue-700 px-6 py-2 rounded font-medium disabled:opacity-50",
                disabled: loading(),
                onclick: move |_| do_submit(),
                if loading() { "Validating..." } else { "Connect" }
            }

            if let Some(err) = error.read().as_ref() {
                p { class: "text-red-400 text-sm", "{err}" }
            }
        }
    }
}

#[component]
fn BotDashboard() -> Element {
    let api_key = use_context::<Signal<ManifoldKey>>();
    let xai_key = use_context::<Signal<XaiKey>>();
    let user_info = use_context::<Signal<Option<api::User>>>();
    let mut connection_status = use_context::<Signal<ConnectionStatus>>();
    let mut log_entries = use_context::<Signal<Vec<BotLogEntry>>>();
    let mut ws_events = use_context::<Signal<Vec<String>>>();

    let mut started = use_signal(|| false);
    if !started() {
        started.set(true);
        let mkey = api_key.read().0.clone();
        let xkey = xai_key.read().0.clone();
        spawn(async move {
            connection_status.set(ConnectionStatus::Connecting);

            let manifold = api::ManifoldClient::new(mkey);
            let xai = xai::XaiClient::new(xkey);

            let (ws_internal_tx, mut ws_internal_rx) = mpsc::unbounded_channel::<ws::WsEvent>();
            let (ws_to_bot_tx, ws_to_bot_rx) = mpsc::unbounded_channel::<ws::WsEvent>();
            let (bot_log_tx, mut bot_log_rx) = mpsc::unbounded_channel::<BotLogEntry>();

            tokio::spawn(ws::run_ws(ws_internal_tx));

            let config = bot::BotConfig::default();
            tokio::spawn(bot::run_bot(manifold, xai, ws_to_bot_rx, bot_log_tx, config));

            loop {
                tokio::select! {
                    Some(event) = ws_internal_rx.recv() => {
                        match &event {
                            ws::WsEvent::Connected => {
                                connection_status.set(ConnectionStatus::Connected);
                            }
                            ws::WsEvent::Disconnected => {
                                connection_status.set(ConnectionStatus::Connecting);
                            }
                            ws::WsEvent::NewContract(b) => {
                                ws_events.write().push(format!(
                                    "New market: \"{}\" by {} [{}]",
                                    b.contract.question, b.creator.username, b.contract.outcome_type
                                ));
                            }
                            ws::WsEvent::NewBet(b) => {
                                ws_events.write().push(format!(
                                    "New bet: market {} (prob {:.0}% → {:.0}%)",
                                    &b.contract_id[..8.min(b.contract_id.len())],
                                    b.prob_before * 100.0,
                                    b.prob_after * 100.0,
                                ));
                            }
                            ws::WsEvent::Error(e) => {
                                ws_events.write().push(format!("Error: {e}"));
                            }
                        };
                        let len = ws_events.read().len();
                        if len > 200 {
                            ws_events.write().drain(0..len - 200);
                        }
                        let _ = ws_to_bot_tx.send(event);
                    }
                    Some(entry) = bot_log_rx.recv() => {
                        log_entries.write().push(entry);
                        let len = log_entries.read().len();
                        if len > 200 {
                            log_entries.write().drain(0..len - 200);
                        }
                    }
                    else => break,
                }
            }
        });
    }

    let user = user_info.read();
    let user = user.as_ref().unwrap();
    let status_text = match connection_status() {
        ConnectionStatus::Disconnected => "Disconnected",
        ConnectionStatus::Connecting => "Connecting...",
        ConnectionStatus::Connected => "Connected",
    };
    let status_color = match connection_status() {
        ConnectionStatus::Disconnected => "text-red-400",
        ConnectionStatus::Connecting => "text-yellow-400",
        ConnectionStatus::Connected => "text-green-400",
    };

    rsx! {
        div { class: "bg-gray-800 rounded-lg p-4 mb-4 flex justify-between items-center",
            div {
                span { class: "text-gray-400", "User: " }
                span { class: "font-medium", "{user.name}" }
                span { class: "text-gray-400 ml-4", "Balance: " }
                span { class: "font-medium text-green-400", "M${user.balance:.0}" }
            }
            div {
                span { class: "text-gray-400", "Status: " }
                span { class: "{status_color} font-medium", "{status_text}" }
            }
        }

        div { class: "grid grid-cols-2 gap-4",
            EventFeed {}
            TradeLog {}
        }
    }
}

#[component]
fn EventFeed() -> Element {
    let ws_events = use_context::<Signal<Vec<String>>>();
    let events = ws_events.read();

    rsx! {
        div { class: "bg-gray-800 rounded-lg p-4",
            h3 { class: "text-lg font-semibold mb-3", "Event Feed" }
            div { class: "space-y-1 max-h-96 overflow-y-auto font-mono text-xs",
                if events.is_empty() {
                    p { class: "text-gray-500", "Waiting for events..." }
                }
                for (i, event) in events.iter().enumerate().rev() {
                    div {
                        key: "{i}",
                        class: "text-gray-300 py-0.5 border-b border-gray-700",
                        "{event}"
                    }
                }
            }
        }
    }
}

/// Split log text into segments, rendering URLs as clickable links.
fn render_log_text(text: &str) -> Element {
    let mut segments: Vec<Element> = Vec::new();
    let mut rest = text;

    while let Some(start) = rest.find("https://") .or_else(|| rest.find("http://")) {
        if start > 0 {
            let before = &rest[..start];
            segments.push(rsx! { "{before}" });
        }
        let url_rest = &rest[start..];
        let end = url_rest
            .find(|c: char| c.is_whitespace() || c == ')' || c == ']' || c == '>' || c == '"')
            .unwrap_or(url_rest.len());
        let url = &url_rest[..end];
        let url_owned = url.to_string();
        segments.push(rsx! {
            a {
                href: "{url_owned}",
                target: "_blank",
                class: "underline text-blue-400 hover:text-blue-300",
                "{url_owned}"
            }
        });
        rest = &url_rest[end..];
    }

    if !rest.is_empty() {
        segments.push(rsx! { "{rest}" });
    }

    rsx! {
        span {
            for seg in segments {
                {seg}
            }
        }
    }
}

#[component]
fn TradeLog() -> Element {
    let log_entries = use_context::<Signal<Vec<BotLogEntry>>>();
    let entries = log_entries.read();

    rsx! {
        div { class: "bg-gray-800 rounded-lg p-4",
            h3 { class: "text-lg font-semibold mb-3", "Bot Log" }
            div { class: "space-y-1 max-h-96 overflow-y-auto font-mono text-xs",
                if entries.is_empty() {
                    p { class: "text-gray-500", "No log entries yet..." }
                }
                for (i, entry) in entries.iter().enumerate().rev() {
                    div {
                        key: "{i}",
                        class: match entry {
                            BotLogEntry::Info(_) => "text-gray-300 py-0.5 border-b border-gray-700",
                            BotLogEntry::Trade(_) => "text-green-400 py-0.5 border-b border-gray-700",
                            BotLogEntry::Error(_) => "text-red-400 py-0.5 border-b border-gray-700",
                        },
                        {render_log_text(match entry {
                            BotLogEntry::Info(s) => s.as_str(),
                            BotLogEntry::Trade(s) => s.as_str(),
                            BotLogEntry::Error(s) => s.as_str(),
                        })}
                    }
                }
            }
        }
    }
}
