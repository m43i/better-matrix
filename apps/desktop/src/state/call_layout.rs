//! Call geometry, independent of Rune and media delivery. The same plan places tiles and
//! decides which videos the desktop watches.

const GAP: f32 = 12.0;
const ASPECT: f32 = 16.0 / 9.0;
const MIN_WIDTH: f32 = 160.0;
const MIN_HEIGHT: f32 = 90.0;
const MAX_TILES: usize = 25;

/// A participant's surface, independent of a camera restarting with another video ID.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum CallTile {
    Camera(String),
    Screen(String),
}

impl CallTile {
    pub fn participant(&self) -> &str {
        match self {
            Self::Camera(key) | Self::Screen(key) => key,
        }
    }

    pub fn screen(&self) -> bool {
        matches!(self, Self::Screen(_))
    }
}

/// Focus without a pin follows the speaker; a pin overrides automatic selection locally.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum CallLayout {
    #[default]
    Auto,
    Grid,
    Focus(Option<CallTile>),
}

/// A corner of the remote video, where a one-to-one call's self-view floats.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    #[default]
    BottomRight,
}

impl Corner {
    pub fn left(self) -> bool {
        matches!(self, Self::TopLeft | Self::BottomLeft)
    }

    pub fn top(self) -> bool {
        matches!(self, Self::TopLeft | Self::TopRight)
    }

    /// The corner of `rect` nearest `point`: where a self-view dropped there snaps to.
    pub fn nearest(rect: [f32; 4], point: [f32; 2]) -> Self {
        let [x, y, w, h] = rect;
        match (point[0] < x + w / 2.0, point[1] < y + h / 2.0) {
            (true, true) => Self::TopLeft,
            (false, true) => Self::TopRight,
            (true, false) => Self::BottomLeft,
            (false, false) => Self::BottomRight,
        }
    }
}

/// The smallest and largest self-view, as a share of the remote video's width.
pub const SELF_VIEW_SCALE: std::ops::RangeInclusive<f32> = 0.15..=0.45;
/// Narrower than this, the self-view shows too little to be worth it.
const SELF_VIEW_MIN_WIDTH: f32 = 96.0;

/// Where a one-to-one call's self-view floats over the remote video, and how large it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SelfView {
    pub corner: Corner,
    /// Its width as a share of the remote video's, within [`SELF_VIEW_SCALE`].
    pub scale: f32,
}

impl Default for SelfView {
    fn default() -> Self {
        Self { corner: Corner::default(), scale: 0.25 }
    }
}

impl SelfView {
    /// A scale the self-view may take.
    pub fn clamp(scale: f32) -> f32 {
        if scale.is_finite() { scale.clamp(*SELF_VIEW_SCALE.start(), *SELF_VIEW_SCALE.end()) } else { 0.25 }
    }

    /// The self-view's rectangle in the corner of `main` (the remote video), for a camera of
    /// `aspect`: never more than half the video either way, nor outside it.
    pub fn place(self, main: [f32; 4], aspect: f32) -> [f32; 4] {
        let [x, y, w, h] = main;
        let aspect = if aspect.is_finite() && aspect > 0.0 { aspect } else { ASPECT };
        let room = [(w - 2.0 * GAP).max(0.0), (h - 2.0 * GAP).max(0.0)];
        let width = (w * Self::clamp(self.scale))
            .max(SELF_VIEW_MIN_WIDTH)
            .min(w * 0.5)
            .min(h * 0.5 * aspect)
            .min(room[0])
            .min(room[1] * aspect);
        let height = width / aspect;
        let left = if self.corner.left() { x + GAP } else { x + w - GAP - width };
        let top = if self.corner.top() { y + GAP } else { y + h - GAP - height };
        // A video smaller than the margins keeps it inside all the same.
        [left.clamp(x, (x + w - width).max(x)), top.clamp(y, (y + h - height).max(y)), width, height]
    }
}

/// The shares of the viewport the user set: the participant strip's and the self-view's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Proportions {
    /// The participant strip's share of the viewport, 0.18 to 0.4.
    pub strip: f32,
    pub self_view: SelfView,
}

impl Default for Proportions {
    fn default() -> Self {
        Self { strip: 0.24, self_view: SelfView::default() }
    }
}

#[derive(Clone, Debug)]
pub struct TileGeometry {
    pub key: CallTile,
    pub aspect: f32,
}

#[derive(Clone, Debug)]
pub struct PlacedTile {
    pub key: CallTile,
    pub rect: [f32; 4],
    pub inset: bool,
}

#[derive(Debug, Default)]
pub struct CallPlan {
    pub tiles: Vec<PlacedTile>,
    pub page: usize,
    pub pages: usize,
    /// The draggable boundary between the main view and the participant strip.
    pub divider: Option<[f32; 4]>,
    pub strip_beside: bool,
}

impl CallPlan {
    pub fn single(key: CallTile, size: [f32; 2], aspect: f32) -> Self {
        Self {
            tiles: vec![PlacedTile { key, rect: fitted([0.0, 0.0, size[0], size[1]], aspect), inset: false }],
            pages: 1,
            ..Default::default()
        }
    }
}

/// The best equal-cell grid, without allocating candidate rectangles.
fn grid_shape(count: usize, bounds: [f32; 4], aspect: f32) -> (usize, [f32; 2]) {
    let [_, _, w, h] = bounds;
    let mut best = (1, [0.0, 0.0]);
    for columns in 1..=count {
        let rows = count.div_ceil(columns);
        let width = ((w - GAP * (columns - 1) as f32) / columns as f32).max(0.0);
        let height = ((h - GAP * (rows - 1) as f32) / rows as f32).max(0.0);
        let width = width.min(height * aspect);
        let height = width / aspect;
        if width * height > best.1[0] * best.1[1] {
            best = (columns, [width, height]);
        }
    }
    best
}

/// A bounded grid with equally sized cells and a centred incomplete last row.
fn cells(count: usize, bounds: [f32; 4], aspect: f32) -> Vec<[f32; 4]> {
    if count == 0 {
        return Vec::new();
    }
    let [x, y, w, h] = bounds;
    let (columns, [width, height]) = grid_shape(count, bounds, aspect);
    let rows = count.div_ceil(columns);
    let top = y + (h - rows as f32 * height - (rows - 1) as f32 * GAP) / 2.0;
    (0..count)
        .map(|i| {
            let row = i / columns;
            let n = columns.min(count - row * columns);
            let left = x + (w - n as f32 * width - (n - 1) as f32 * GAP) / 2.0;
            [left + (i % columns) as f32 * (width + GAP), top + row as f32 * (height + GAP), width, height]
        })
        .collect()
}

fn gallery(tiles: &[&TileGeometry], bounds: [f32; 4], page: usize, voice: bool) -> CallPlan {
    if tiles.is_empty() {
        return CallPlan::default();
    }
    let aspect = if voice { 1.2 } else { ASPECT };
    let minimum = if voice { [104.0, 80.0] } else { [MIN_WIDTH, MIN_HEIGHT] };
    let capacity = (1..=tiles.len().min(MAX_TILES))
        .rev()
        .find(|count| {
            let (_, [w, h]) = grid_shape(*count, bounds, aspect);
            w >= minimum[0] && h >= minimum[1]
        })
        .unwrap_or(1);
    let pages = tiles.len().div_ceil(capacity);
    let page = page.min(pages - 1);
    let shown = &tiles[page * capacity..tiles.len().min((page + 1) * capacity)];
    let rects = cells(shown.len(), bounds, aspect);
    CallPlan {
        tiles: shown
            .iter()
            .zip(rects)
            .map(|(tile, rect)| PlacedTile { key: tile.key.clone(), rect, inset: false })
            .collect(),
        page,
        pages,
        ..Default::default()
    }
}

fn fitted(bounds: [f32; 4], aspect: f32) -> [f32; 4] {
    let [x, y, w, h] = bounds;
    let aspect = if aspect.is_finite() && aspect > 0.0 { aspect } else { ASPECT };
    let width = w.min(h * aspect);
    let height = width / aspect;
    [x + (w - width) / 2.0, y + (h - height) / 2.0, width, height]
}

/// All sizes are points in the tile viewport; controls and paging live outside this rectangle.
pub fn plan(
    tiles: &[TileGeometry],
    size: [f32; 2],
    layout: &CallLayout,
    speaker: Option<&str>,
    page: usize,
    proportions: Proportions,
    voice: bool,
) -> CallPlan {
    let [w, h] = size;
    if !w.is_finite() || !h.is_finite() || w <= 0.0 || h <= 0.0 {
        return CallPlan::default();
    }
    let bounds = [0.0, 0.0, w, h];
    let all: Vec<_> = tiles.iter().collect();
    if voice || matches!(layout, CallLayout::Grid) {
        return gallery(&all, bounds, page, voice);
    }
    let screen = tiles.iter().find(|tile| tile.key.screen());
    let remote = tiles.iter().find(|tile| !tile.key.screen() && !tile.key.participant().is_empty());
    let main = match layout {
        CallLayout::Focus(pin) => pin
            .as_ref()
            .and_then(|key| tiles.iter().find(|tile| tile.key == *key))
            .or_else(|| {
                speaker.and_then(|key| tiles.iter().find(|tile| !tile.key.screen() && tile.key.participant() == key))
            })
            .or(remote)
            .or(screen)
            .or_else(|| tiles.first()),
        CallLayout::Auto => screen.or_else(|| {
            let remote_count =
                tiles.iter().filter(|tile| !tile.key.screen() && !tile.key.participant().is_empty()).count();
            (remote_count == 1).then_some(remote).flatten()
        }),
        CallLayout::Grid => None,
    };
    let Some(main) = main else { return gallery(&all, bounds, page, false) };
    let others: Vec<_> = tiles.iter().filter(|tile| tile.key != main.key).collect();
    if others.is_empty() {
        return CallPlan::single(main.key.clone(), size, main.aspect);
    }
    // A one-to-one camera call keeps a small self-view in a corner of the remote video, which
    // the user moves and resizes.
    if !main.key.screen() && others.len() == 1 && others[0].key == CallTile::Camera(String::new()) {
        let main_rect = fitted(bounds, main.aspect);
        return CallPlan {
            tiles: vec![
                PlacedTile { key: main.key.clone(), rect: main_rect, inset: false },
                PlacedTile {
                    key: others[0].key.clone(),
                    rect: proportions.self_view.place(main_rect, others[0].aspect),
                    inset: true,
                },
            ],
            pages: 1,
            ..Default::default()
        };
    }
    let beside = w / h >= 1.5 && w >= 640.0;
    let fraction = proportions.strip.clamp(0.18, 0.4);
    let (main_bounds, strip_bounds, divider) = if beside {
        let width = (w * fraction).max(160.0).min(w * 0.4);
        let edge = w - width - GAP;
        ([0.0, 0.0, edge, h], [w - width, 0.0, width, h], [edge, 0.0, GAP, h])
    } else {
        let height = (h * fraction).max(90.0).min(h * 0.4);
        let edge = (h - height - GAP).max(0.0);
        ([0.0, 0.0, w, edge], [0.0, h - height, w, height], [0.0, edge, w, GAP])
    };
    let mut result = gallery(&others, strip_bounds, page, false);
    result.tiles.insert(0, PlacedTile { key: main.key.clone(), rect: fitted(main_bounds, main.aspect), inset: false });
    result.divider = Some(divider);
    result.strip_beside = beside;
    result
}

#[cfg(test)]
mod tests;
