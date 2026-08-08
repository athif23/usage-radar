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
        raise SystemExit(f"view overlay target not found in {path}: {old[:100]!r}")
    file.write_text(text.replace(old, new))

patch(
    "src/app/view.rs",
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
)
patch(
    "src/app/view.rs",
    """                    .child(div().flex_1())
                    .child(self.icon_button(
                        "toolbar-quit",
""",
    """                    .child(
                        div()
                            .id("panel-drag-handle")
                            .flex_1()
                            .h(px(32.0))
                            .on_mouse_down(
                                MouseButton::Left,
                                |_: &MouseDownEvent, window, _| {
                                    window.start_window_move();
                                },
                            ),
                    )
                    .child(self.icon_button(
                        "toolbar-quit",
""",
)
patch("src/panel/mod.rs", "        is_movable: false,", "        is_movable: true,")
PY
