use std::time::{Duration, Instant};

use iced::window;
use iced::{Point, Size};
use tray_icon::Rect;

use crate::providers::ProviderKind;

pub const WIDTH: f32 = 360.0;
pub const HEIGHT: f32 = 580.0;
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

pub fn open_point(anchor: Option<Rect>, scale_factor: f32) -> Option<Point> {
    #[cfg(target_os = "windows")]
    if let Some(point) = windows_work_area_point(scale_factor) {
        return Some(point);
    }

    anchor.map(|rect| anchor_point(rect, scale_factor))
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

#[cfg(target_os = "windows")]
fn windows_work_area_point(scale_factor: f32) -> Option<Point> {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW, SPI_GETWORKAREA};

    let scale = scale_factor.max(0.25);
    let mut work_area = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };

    let ok = unsafe { SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut work_area as *mut _ as _, 0) };
    if ok == 0 {
        return None;
    }

    let right = work_area.right as f32 / scale;
    let bottom = work_area.bottom as f32 / scale;
    let left = work_area.left as f32 / scale;
    let top = work_area.top as f32 / scale;

    let x = (right - WIDTH - EDGE_MARGIN).max(left + EDGE_MARGIN);
    let y = (bottom - HEIGHT - EDGE_MARGIN).max(top + EDGE_MARGIN);

    Some(Point::new(x, y))
}

fn default_position(visible: bool) -> window::Position {
    if visible {
        window::Position::Centered
    } else {
        window::Position::Default
    }
}
