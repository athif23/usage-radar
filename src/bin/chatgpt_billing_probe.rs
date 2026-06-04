#[cfg(target_os = "windows")]
fn main() {
    if let Err(error) = windows_probe::run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("ChatGPT billing probe is only available on Windows.");
    std::process::exit(1);
}

#[cfg(target_os = "windows")]
mod windows_probe {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use serde::Serialize;
    use tao::dpi::{LogicalPosition, LogicalSize};
    use tao::event::{Event, WindowEvent};
    use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
    use tao::window::WindowBuilder;
    use wry::{http::Request, WebContext, WebViewBuilder};

    const BILLING_URL: &str = "https://chatgpt.com/admin/billing";
    const DEFAULT_TIMEOUT_SECONDS: u64 = 25;

    enum UserEvent {
        Save(ResultPayload),
        Timeout,
    }

    #[derive(Debug, Serialize)]
    struct ResultPayload {
        remaining: f64,
        captured_at_epoch_seconds: u64,
        source: String,
    }

    pub fn run() -> Result<(), String> {
        let options = ProbeOptions::from_args();
        let result_path = result_path()?;
        let data_dir = webview_data_dir()?;
        fs::create_dir_all(&data_dir).map_err(|error| {
            format!(
                "Failed to create WebView data directory {}: {error}",
                data_dir.display()
            )
        })?;

        let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
        let proxy = event_loop.create_proxy();
        let mut window_builder = WindowBuilder::new().with_title("Usage Radar - ChatGPT Billing");
        window_builder = if options.background {
            window_builder
                .with_inner_size(LogicalSize::new(1.0, 1.0))
                .with_position(LogicalPosition::new(-32_000.0, -32_000.0))
                .with_decorations(false)
                .with_visible(false)
        } else {
            window_builder
                .with_inner_size(LogicalSize::new(1100.0, 760.0))
                .with_visible(true)
        };
        let window = window_builder
            .build(&event_loop)
            .map_err(|error| format!("Failed to open ChatGPT billing window: {error}"))?;

        if let Some(timeout) = options.timeout {
            schedule_timeout(proxy.clone(), timeout);
        }

        let mut context = WebContext::new(Some(data_dir));
        let script = probe_script();
        let handler = ipc_handler(proxy);
        let _webview = WebViewBuilder::new_with_web_context(&mut context)
            .with_url(BILLING_URL)
            .with_initialization_script(script)
            .with_ipc_handler(handler)
            .build(&window)
            .map_err(|error| format!("Failed to create ChatGPT billing WebView: {error}"))?;

        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Wait;

            match event {
                Event::UserEvent(UserEvent::Save(payload)) => {
                    if let Ok(contents) = serde_json::to_string_pretty(&payload) {
                        if let Some(parent) = result_path.parent() {
                            let _ = fs::create_dir_all(parent);
                        }
                        let _ = fs::write(&result_path, contents);
                    }
                    *control_flow = ControlFlow::Exit;
                }
                Event::UserEvent(UserEvent::Timeout) => {
                    eprintln!("ChatGPT billing balance was not found before timeout.");
                    *control_flow = ControlFlow::Exit;
                }
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                } => {
                    *control_flow = ControlFlow::Exit;
                }
                _ => {}
            }
        });
    }

    struct ProbeOptions {
        background: bool,
        timeout: Option<Duration>,
    }

    impl ProbeOptions {
        fn from_args() -> Self {
            let mut background = false;
            let mut timeout_seconds = DEFAULT_TIMEOUT_SECONDS;
            let mut args = std::env::args().skip(1);

            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--background" => background = true,
                    "--timeout-seconds" => {
                        if let Some(value) = args.next().and_then(|value| value.parse().ok()) {
                            timeout_seconds = value;
                        }
                    }
                    _ => {}
                }
            }

            Self {
                background,
                timeout: background.then_some(Duration::from_secs(timeout_seconds)),
            }
        }
    }

    fn schedule_timeout(proxy: EventLoopProxy<UserEvent>, timeout: Duration) {
        std::thread::spawn(move || {
            std::thread::sleep(timeout);
            let _ = proxy.send_event(UserEvent::Timeout);
        });
    }

    fn ipc_handler(proxy: EventLoopProxy<UserEvent>) -> impl Fn(Request<String>) + 'static {
        move |request: Request<String>| {
            let Ok(message) = serde_json::from_str::<ProbeMessage>(request.body()) else {
                return;
            };

            if message.kind != "billing-balance" {
                return;
            }

            let Some(remaining) = parse_credit_number(&message.value) else {
                return;
            };

            let captured_at_epoch_seconds = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::ZERO)
                .as_secs();

            let _ = proxy.send_event(UserEvent::Save(ResultPayload {
                remaining,
                captured_at_epoch_seconds,
                source: "ChatGPT billing WebView".to_string(),
            }));
        }
    }

    #[derive(Debug, serde::Deserialize)]
    struct ProbeMessage {
        #[serde(rename = "type")]
        kind: String,
        value: String,
    }

    fn probe_script() -> &'static str {
        r#"
(() => {
  if (window.__usageRadarBillingProbeInstalled) return;
  window.__usageRadarBillingProbeInstalled = true;

  const normalize = (text) => (text || '').replace(/\s+/g, ' ').trim();
  const send = (value) => {
    if (!value) return;
    window.ipc.postMessage(JSON.stringify({ type: 'billing-balance', value }));
  };

  const findBalance = () => {
    const body = normalize(document.body ? document.body.innerText : '');
    const labels = [
      'Saldo Kredit',
      'Credit balance',
      'Credits balance',
      'Remaining credits',
      'Credits remaining'
    ];

    for (const label of labels) {
      const index = body.toLowerCase().indexOf(label.toLowerCase());
      if (index < 0) continue;
      const tail = body.slice(index + label.length, index + label.length + 180);
      const match = tail.match(/([0-9][0-9.,\s]*)/);
      if (match) return match[1];
    }
    return null;
  };

  const tick = () => {
    try { send(findBalance()); } catch (_) {}
  };

  setInterval(tick, 1500);
  window.addEventListener('load', tick);
  document.addEventListener('readystatechange', tick);
  tick();
})();
"#
    }

    fn result_path() -> Result<PathBuf, String> {
        local_app_data_dir().map(|dir| dir.join("UsageRadar").join("chatgpt_billing.json"))
    }

    fn webview_data_dir() -> Result<PathBuf, String> {
        local_app_data_dir().map(|dir| dir.join("UsageRadar").join("ChatGPTWebView"))
    }

    fn local_app_data_dir() -> Result<PathBuf, String> {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .ok_or_else(|| "LOCALAPPDATA is not set.".to_string())
    }

    fn parse_credit_number(raw: &str) -> Option<f64> {
        let token = raw
            .trim()
            .replace([' ', '\u{00A0}', '\u{202F}'], "")
            .trim_matches(|character: char| {
                !character.is_ascii_digit() && character != '.' && character != ','
            })
            .to_string();

        if token.is_empty() {
            return None;
        }

        let normalized = if token.contains('.') && token.contains(',') {
            let last_dot = token.rfind('.').unwrap_or(0);
            let last_comma = token.rfind(',').unwrap_or(0);
            if last_dot > last_comma {
                token.replace(',', "")
            } else {
                token.replace('.', "").replace(',', ".")
            }
        } else if token.contains('.') {
            if looks_like_grouped_number(&token, '.') {
                token.replace('.', "")
            } else {
                token
            }
        } else if token.contains(',') {
            if looks_like_grouped_number(&token, ',') {
                token.replace(',', "")
            } else {
                token.replace(',', ".")
            }
        } else {
            token
        };

        normalized.parse::<f64>().ok()
    }

    fn looks_like_grouped_number(token: &str, separator: char) -> bool {
        let parts = token.split(separator).collect::<Vec<_>>();
        parts.len() > 1
            && parts[0].chars().all(|character| character.is_ascii_digit())
            && (1..=3).contains(&parts[0].len())
            && parts[1..].iter().all(|part| {
                part.len() == 3 && part.chars().all(|character| character.is_ascii_digit())
            })
    }
}
