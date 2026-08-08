#!/usr/bin/env bash
set -euo pipefail
project=${1:?project path required}
python - "$project" <<'PY'
from pathlib import Path
import sys
root = Path(sys.argv[1])
p = root / "src/app/mod.rs"
s = p.read_text()
s = s.replace(
    "use gpui::{actions, App as GpuiApp, ClipboardItem, Context, KeyBinding};",
    "use gpui::{actions, App as GpuiApp, AppContext, ClipboardItem, Context, KeyBinding};",
)
p.write_text(s)
p = root / "src/main.rs"
s = p.read_text()
s = s.replace(
    "use gpui::App as GpuiApp;",
    "use gpui::{App as GpuiApp, AppContext};",
)
p.write_text(s)
PY
