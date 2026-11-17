//! Session-local pane widths, constrained to leave room for the conversation.

use rune::{Drag, Phase};

const ROOM_MIN: f32 = 320.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pane {
    Sidebar,
    SidePanel,
    Thread,
}

impl Pane {
    fn limits(self) -> (f32, f32) {
        match self {
            Self::Sidebar => (220.0, 420.0),
            Self::SidePanel | Self::Thread => (300.0, 640.0),
        }
    }

    fn direction(self) -> f32 {
        match self {
            Self::Sidebar => 1.0,
            Self::SidePanel | Self::Thread => -1.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PaneSize {
    pub width: f32,
    pub min: f32,
    pub max: f32,
}

impl PaneSize {
    fn new(pane: Pane, preferred: f32, available: f32) -> Self {
        let (min, max) = pane.limits();
        let max = available.clamp(0.0, max);
        let min = min.min(max);
        Self { width: preferred.clamp(min, max), min, max }
    }

    pub fn clamp(self, width: f32) -> f32 {
        width.clamp(self.min, self.max)
    }
}

pub struct PaneLayout {
    pub sidebar: Option<PaneSize>,
    /// Whether there is room to show the sidebar, independently of the call's preference.
    pub sidebar_available: bool,
    pub right: Option<(Pane, PaneSize)>,
}

#[derive(Debug, Default)]
enum Resize {
    #[default]
    Idle,
    Dragging {
        pane: Pane,
        start: f32,
        previous: f32,
    },
}

#[derive(Debug)]
pub struct PaneWidths {
    sidebar: f32,
    side: f32,
    thread: f32,
    resize: Resize,
}

impl Default for PaneWidths {
    fn default() -> Self {
        Self { sidebar: 260.0, side: 320.0, thread: 420.0, resize: Resize::Idle }
    }
}

impl PaneWidths {
    /// `available` excludes the fixed rail. The sidebar yields in narrow windows while a
    /// right-hand panel is open; preferred widths return when there is room again.
    pub fn layout(&self, available: f32, sidebar: bool, right: Option<Pane>) -> PaneLayout {
        let available = available.max(0.0);
        let sidebar_min = Pane::Sidebar.limits().0;
        let right_min = right.map_or(0.0, |pane| pane.limits().0);
        let sidebar_available = right.is_none() || available >= sidebar_min + right_min + ROOM_MIN;
        let sidebar = (sidebar && sidebar_available).then(|| {
            PaneSize::new(
                Pane::Sidebar,
                self.sidebar,
                (available - right_min - ROOM_MIN).max(sidebar_min).min(available),
            )
        });
        // Keep the panel readable even when zoom leaves too little space for ROOM_MIN.
        let right = right.map(|pane| {
            let space = (available - sidebar.map_or(0.0, |size| size.width) - ROOM_MIN).max(right_min).min(available);
            (pane, PaneSize::new(pane, self.preferred(pane), space))
        });
        PaneLayout { sidebar, sidebar_available, right }
    }

    fn preferred(&self, pane: Pane) -> f32 {
        match pane {
            Pane::Sidebar => self.sidebar,
            Pane::SidePanel => self.side,
            Pane::Thread => self.thread,
        }
    }

    pub fn set(&mut self, pane: Pane, width: f32) {
        let (min, max) = pane.limits();
        let width = width.clamp(min, max);
        match pane {
            Pane::Sidebar => self.sidebar = width,
            Pane::SidePanel => self.side = width,
            Pane::Thread => self.thread = width,
        }
    }

    pub fn drag(&mut self, pane: Pane, size: PaneSize, drag: &Drag) {
        match drag.phase {
            Phase::Down => {
                self.resize = Resize::Dragging { pane, start: size.width, previous: self.preferred(pane) };
            }
            Phase::Move | Phase::Up => {
                if let Resize::Dragging { pane: active, start, previous } = self.resize
                    && active == pane
                {
                    let resized = size.clamp(start + pane.direction() * drag.dx);
                    let width = if drag.cancelled() || resized == start { previous } else { resized };
                    self.set(pane, width);
                }
                if drag.phase == Phase::Up {
                    self.resize = Resize::Idle;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
