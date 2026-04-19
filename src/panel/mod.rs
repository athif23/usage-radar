use std::time::{Duration, Instant};

use iced::window;
use iced::{Point, Size};
use tray_icon::Rect;

use crate::providers::ProviderKind;

pub const WIDTH: f32 = 480.0;
pub const HEIGHT: f32 = 720.0;
const EDGE_MARGIN: f32 = 8.0;

#[derive(Debug, Clone)]
pub struct State {
    pub id: Option<window::Id>,
    pub visible: bool,
    pub scale_factor: f32,
    pub anchor: Option<Rect>,
    pub selected_provider: Option<ProviderKind>,
    pub show_about: bool,
    pub last_scrolled_at: Option<Instant>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            id: None,
            visible: false,
            scale_factor: 1.0,
            anchor: None,
            selected_provider: None,
            show_about: false,
            last_scrolled_at: None,
        }
    }
}

impl State {
    pub fn note_scrolled(&mut self) {
        self.last_scrolled_at = Some(Instant::now());
    }

    pub fn scrollbar_is_active(&self) -> bool {
        self.last_scrolled_at
            .map(|last| last.elapsed() <= Duration::from_millis(1400))
            .unwrap_or(false)
    }
}

pub fn settings(visible: bool, skip_taskbar: bool, position: Option<Point>) -> window::Settings {
    window::Settings {
        size: Size::new(WIDTH, HEIGHT),
        position: position
            .map(window::Position::Specific)
            .unwrap_or_else(|| default_position(visible)),
        min_size: Some(Size::new(WIDTH, HEIGHT)),
        max_size: Some(Size::new(WIDTH, HEIGHT)),
        visible,
        resizable: false,
        decorations: false,
        transparent: false,
        level: window::Level::Normal,
        exit_on_close_request: false,
        platform_specific: window::settings::PlatformSpecific {
            skip_taskbar,
            undecorated_shadow: false,
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn anchor_point(rect: Rect, scale_factor: f32) -> Point {
    let scale = scale_factor.max(0.25);
    let left = rect.position.x as f32 / scale;
    let top = rect.position.y as f32 / scale;
    let width = rect.size.width as f32 / scale;
    let height = rect.size.height as f32 / scale;

    let mut x = left + width - WIDTH;
    if x < EDGE_MARGIN {
        x = EDGE_MARGIN;
    }

    let above = top - HEIGHT - EDGE_MARGIN;
    let below = top + height + EDGE_MARGIN;
    let y = if above >= EDGE_MARGIN {
        above
    } else {
        below.max(EDGE_MARGIN)
    };

    Point::new(x, y)
}

fn default_position(visible: bool) -> window::Position {
    if visible {
        window::Position::Centered
    } else {
        window::Position::Default
    }
}
