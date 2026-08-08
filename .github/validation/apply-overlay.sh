#!/usr/bin/env bash
set -euo pipefail
project=${1:?project path required}
python - "$project" <<'PY'
from pathlib import Path
import sys
root = Path(sys.argv[1])

def patch(path: str, old: str, new: str) -> None:
    file = root / path
    text = file.read_text()
    if old not in text:
        raise SystemExit(f"overlay target not found in {path}: {old[:80]!r}")
    file.write_text(text.replace(old, new))

patch(
    "src/app/mod.rs",
    "use gpui::{actions, App as GpuiApp, ClipboardItem, Context, KeyBinding};",
    "use gpui::{actions, AppContext, ClipboardItem, Context, KeyBinding};",
)
patch(
    "src/app/mod.rs",
    "use crate::providers::{self, Confidence, ProviderKind, ProviderSnapshot};",
    "use crate::providers::{self, ProviderKind, ProviderSnapshot};",
)
patch(
    "src/app/mod.rs",
    """        self.shell_task = Some(cx.spawn(async move |this, cx| loop {
            cx.background_spawn(async {
                std::thread::sleep(Duration::from_millis(75));
            })
            .await;

            let keep_running = this""",
    """        self.shell_task = Some(cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(75))
                .await;

            let keep_running = this""",
)
patch(
    "src/app/mod.rs",
    "    pub(super) fn provider_card_model(&self, kind: ProviderKind) -> ProviderCardModel {",
    "    fn provider_card_model(&self, kind: ProviderKind) -> ProviderCardModel {",
)
patch(
    "src/app/mod.rs",
    "    pub(super) fn notice_tone(&self) -> Tone {",
    "    fn notice_tone(&self) -> Tone {",
)
patch(
    "src/app/state.rs",
    "pub struct AppGlobal(pub Entity<App>);",
    "pub struct AppGlobal {\n    pub app: Entity<App>,\n}",
)
patch(
    "src/main.rs",
    "use gpui::App as GpuiApp;",
    "use gpui::{App as GpuiApp, AppContext};",
)
patch(
    "src/main.rs",
    "            let app = cx.new(UsageRadarApp::new);\n            cx.set_global(AppGlobal(app.clone()));\n\n            let app_for_close = app.downgrade();",
    "            let app = cx.new(UsageRadarApp::new);\n            cx.set_global(AppGlobal { app });\n            let app = cx.global::<AppGlobal>().app.clone();\n\n            let app_for_close = app.downgrade();",
)
patch("src/app/view.rs", 'const ICON_CHECK: &str = "icons/check.svg";\n', "")
patch(
    "src/providers/codex.rs",
    "use std::path::{Path, PathBuf};",
    "use std::path::PathBuf;\n#[cfg(target_os = \"windows\")]\nuse std::path::Path;",
)
patch(
    "src/providers/codex.rs",
    'const CHATGPT_URL: &str = "https://chatgpt.com";',
    '#[cfg(target_os = "windows")]\nconst CHATGPT_URL: &str = "https://chatgpt.com";',
)
patch(
    "src/providers/opencode_go.rs",
    'const APP_URL: &str = "https://app.opencode.ai";',
    '#[cfg(target_os = "windows")]\nconst APP_URL: &str = "https://app.opencode.ai";',
)
patch(
    "src/providers/opencode_go.rs",
    "    BrowserImport { source_label: String },",
    "    #[cfg(target_os = \"windows\")]\n    BrowserImport { source_label: String },",
)
patch(
    "src/providers/opencode_go.rs",
    "            Self::BrowserImport { source_label } => {",
    "            #[cfg(target_os = \"windows\")]\n            Self::BrowserImport { source_label } => {",
)
patch(
    "src/providers/opencode_go.rs",
    "        CookieSource::BrowserImport { source_label } => format!(",
    "        #[cfg(target_os = \"windows\")]\n        CookieSource::BrowserImport { source_label } => format!(",
)
patch(
    "src/storage/config.rs",
    "\n    pub fn from_u8(value: u8) -> Self {\n        match value {\n            1 => Self::Dark,\n            _ => Self::Light,\n        }\n    }\n",
    "\n",
)
patch("src/theme.rs", "use gpui::{Hsla, hsla, rgb};", "use gpui::{rgb, Hsla};")
for line in [
    "    pub accent_hover: Hsla,\n",
    "    pub accent_active: Hsla,\n",
    "    pub danger_soft: Hsla,\n",
    "    pub success: Hsla,\n",
    "    pub shadow: Hsla,\n",
    "            accent_hover: color(0x2d63ca),\n",
    "            accent_active: color(0x2858b5),\n",
    "            danger_soft: color(0xffecee),\n",
    "            success: color(0x287b50),\n",
    "            shadow: hsla(220.0 / 360.0, 0.18, 0.10, 0.18),\n",
    "            accent_hover: color(0x8fb1fa),\n",
    "            accent_active: color(0x6a92e7),\n",
    "            danger_soft: color(0x402629),\n",
    "            success: color(0x67c995),\n",
    "            shadow: hsla(0.0, 0.0, 0.0, 0.50),\n",
]:
    patch("src/theme.rs", line, "")
PY
