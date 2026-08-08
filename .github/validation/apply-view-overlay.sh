#!/usr/bin/env bash
set -euo pipefail
project=${1:?project path required}
python - "$project" <<'PY'
from pathlib import Path
import sys
root = Path(sys.argv[1])
path = root / "src/app/view.rs"
text = path.read_text()
replacements = [
    (
        "    ElementId, Entity, FocusHandle, Focusable, FontWeight, Hsla, IntoElement, MouseButton,\n    MouseDownEvent, Render, Window,\n",
        "    ElementId, Entity, FocusHandle, Focusable, FontWeight, Hsla, IntoElement, Render, Window,\n",
    ),
    (
        """                div()
                    .id("panel-drag-region")
                    .flex()
                    .items_center()
                    .px(px(7.0))
                    .py(px(5.0))
                    .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, window, _| {
                        window.start_window_move();
                    })
""",
        """                div()
                    .id("panel-bottom-toolbar")
                    .flex()
                    .items_center()
                    .px(px(7.0))
                    .py(px(5.0))
""",
    ),
]
for old, new in replacements:
    if old not in text:
        raise SystemExit(f"view overlay target not found: {old[:100]!r}")
    text = text.replace(old, new)
path.write_text(text)
PY
