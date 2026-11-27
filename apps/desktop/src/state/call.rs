//! The call under way, as the desktop shows it: the engine's [`CallInfo`], how loud everyone is,
//! the image each video streams into, and the call layout and chat display preferences.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use matrix_core::{CallInfo, VideoId};
use rune::Pixels;

use super::State;
use super::call_glow::CallGlow;
use super::call_layout::{self, CallLayout, CallPlan, CallTile, Corner, Proportions, SelfView, TileGeometry};

/// Loudness (0 to 1) from which someone counts as speaking, for their ring on the stage.
pub const SPEAKING: f32 = 0.06;
/// How long someone still counts as speaking after going quiet, so their ring doesn't flicker
/// between words.
const SPEAKING_HOLD: Duration = Duration::from_millis(400);
const SPEAKER_HOLD: Duration = Duration::from_millis(1500);

/// Switching focus needs sustained speech; the speaking rings remain responsive.
#[derive(Debug, Default)]
enum Speaker {
    #[default]
    Idle,
    Steady(String),
    Switching {
        current: Option<String>,
        candidate: String,
        since: Instant,
    },
}

impl Speaker {
    fn current(&self) -> Option<&str> {
        match self {
            Self::Idle => None,
            Self::Steady(key) => Some(key),
            Self::Switching { current, .. } => current.as_deref(),
        }
    }

    fn heard(&mut self, candidate: Option<&str>, now: Instant) -> bool {
        let current = self.current().map(str::to_owned);
        if candidate == current.as_deref() || candidate.is_none() {
            *self = current.map_or(Self::Idle, Self::Steady);
            return false;
        }
        let Some(candidate) = candidate else { return false };
        if let Self::Switching { candidate: pending, since, .. } = self {
            if pending == candidate && now.duration_since(*since) >= SPEAKER_HOLD {
                *self = Self::Steady(candidate.to_owned());
                return true;
            }
            if pending == candidate {
                return false;
            }
        }
        *self = Self::Switching { current, candidate: candidate.to_owned(), since: now };
        false
    }
}

#[derive(Debug)]
pub struct Call {
    pub info: CallInfo,
    /// Each participant's loudness, 0 to 1, by participant key; the user's own under "".
    pub levels: HashMap<String, f32>,
    /// Who is speaking now, by participant key.
    pub speaking: HashSet<String>,
    /// When each participant was last loud enough to count as speaking.
    loud_at: HashMap<String, Instant>,
    /// The engine streams the videos into their images: while the stage shows.
    pub watched: HashSet<VideoId>,
    /// Each video shown, by ID: the image its frames stream into.
    pub videos: HashMap<VideoId, Pixels>,
    /// The room's chat shows alongside the stage.
    pub chat: bool,
    pub hide_sidebar: bool,
    pub layout: CallLayout,
    /// Size of the viewport containing tiles, excluding controls and paging.
    pub size: [f32; 2],
    /// The room body decides whether chat fits beside or below the stage.
    pub room_size: [f32; 2],
    pub page: usize,
    pub show_self: bool,
    pub full_video: HashMap<CallTile, bool>,
    pub strip: f32,
    /// Where a one-to-one call's self-view floats, and how large.
    pub self_view: SelfView,
    /// How far the user has dragged the self-view from its corner, while they drag it.
    pub self_drag: Option<[f32; 2]>,
    aspects: HashMap<VideoId, f32>,
    speaker: Speaker,
    pub glow: CallGlow,
    /// What `watched` was worked out for: whether the stage showed, and [`Call::videos_key`].
    pub watched_for: Option<(bool, u64)>,
}

/// What changed in the videos when the call did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct VideoChanges {
    /// Videos to start watching.
    pub added: Vec<VideoId>,
    /// Videos no longer in the call.
    pub removed: Vec<VideoId>,
}

impl Call {
    pub fn new(info: CallInfo) -> Self {
        Self {
            info,
            levels: HashMap::new(),
            speaking: HashSet::new(),
            loud_at: HashMap::new(),
            watched: HashSet::new(),
            videos: HashMap::new(),
            chat: false,
            hide_sidebar: false,
            layout: CallLayout::Auto,
            size: [800.0, 450.0],
            room_size: [800.0, 600.0],
            page: 0,
            show_self: true,
            full_video: HashMap::new(),
            strip: 0.24,
            self_view: SelfView::default(),
            self_drag: None,
            aspects: HashMap::new(),
            speaker: Speaker::Idle,
            glow: CallGlow::default(),
            watched_for: None,
        }
    }

    /// Every video the call carries now: the user's own previews, then everyone else's.
    fn carried(info: &CallInfo) -> Vec<VideoId> {
        let own = [info.camera, info.screen];
        let others = info.participants.iter().flat_map(|p| [p.camera, p.screen]);
        own.into_iter().chain(others).flatten().collect()
    }

    /// Takes the engine's latest view of the call; reports the videos that came and went, each
    /// added one with a fresh image.
    pub fn update(&mut self, info: CallInfo) -> VideoChanges {
        self.info = info;
        let carried: HashSet<VideoId> = Self::carried(&self.info).into_iter().collect();
        let mut changes = VideoChanges::default();
        for id in &carried {
            if !self.videos.contains_key(id) {
                self.videos.insert(*id, Pixels::new());
                changes.added.push(*id);
            }
        }
        self.videos.retain(|id, _| {
            let keep = carried.contains(id);
            if !keep {
                changes.removed.push(*id);
            }
            keep
        });
        changes.added.sort();
        changes.removed.sort();
        let present = |key: &String| key.is_empty() || self.info.participants.iter().any(|p| p.key == *key);
        self.levels.retain(|key, _| present(key));
        self.loud_at.retain(|key, _| present(key));
        self.speaking.retain(|key| present(key));
        self.aspects.retain(|id, _| carried.contains(id));
        let tiles = self.tiles();
        self.full_video.retain(|key, _| tiles.iter().any(|tile| tile.key == *key));
        if let CallLayout::Focus(Some(pin)) = &self.layout
            && !tiles.iter().any(|tile| tile.key == *pin)
        {
            self.layout = CallLayout::Auto;
        }
        if self.speaker.current().is_some_and(|key| !self.info.participants.iter().any(|p| p.key == key)) {
            self.speaker = Speaker::Idle;
        }
        self.page = self.plan().page;
        changes
    }

    /// Takes everyone's loudness (0 to 255) at `now`. True when who is speaking changed, which
    /// is all the view shows of it; the glow reads the levels on its own.
    pub fn heard(&mut self, levels: Vec<(String, u8)>, now: Instant) -> bool {
        self.levels = levels.into_iter().map(|(key, level)| (key, f32::from(level) / 255.0)).collect();
        for (key, _) in self.levels.iter().filter(|(_, level)| **level >= SPEAKING) {
            self.loud_at.insert(key.clone(), now);
        }
        self.loud_at.retain(|_, at| now.duration_since(*at) < SPEAKING_HOLD);
        let speaking: HashSet<String> = self.loud_at.keys().cloned().collect();
        let changed = speaking != self.speaking;
        self.speaking = speaking;
        let candidate = self
            .info
            .participants
            .iter()
            .filter(|p| !p.muted && self.speaking.contains(&p.key))
            .map(|p| (&p.key, self.levels.get(&p.key).copied().unwrap_or(0.0)))
            .max_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(b.0)))
            .map(|(key, _)| key.as_str());
        self.speaker.heard(candidate, now) || changed
    }

    pub fn video(&self) -> bool {
        self.info.video
            || self.info.camera.is_some()
            || self.info.screen.is_some()
            || self.info.participants.iter().any(|p| p.camera.is_some() || p.screen.is_some())
    }

    pub fn video_id(&self, key: &CallTile) -> Option<VideoId> {
        let (camera, screen) = if key.participant().is_empty() {
            (self.info.camera, self.info.screen)
        } else {
            let person = self.info.participants.iter().find(|p| p.key == key.participant())?;
            (person.camera, person.screen)
        };
        if key.screen() { screen } else { camera }
    }

    pub fn aspect(&self, key: &CallTile) -> f32 {
        self.video_id(key).and_then(|id| self.aspects.get(&id).copied()).unwrap_or(16.0 / 9.0)
    }

    fn tiles(&self) -> Vec<TileGeometry> {
        let cameras = self
            .info
            .participants
            .iter()
            .map(|p| CallTile::Camera(p.key.clone()))
            .chain(self.show_self.then(|| CallTile::Camera(String::new())));
        let screens = self
            .info
            .participants
            .iter()
            .filter(|p| p.screen.is_some())
            .map(|p| CallTile::Screen(p.key.clone()))
            .chain(self.info.screen.map(|_| CallTile::Screen(String::new())));
        screens.chain(cameras).map(|key| TileGeometry { aspect: self.aspect(&key), key }).collect()
    }

    pub fn plan(&self) -> CallPlan {
        let tiles = self.tiles();
        let compact = self.compact() && self.video();
        let plan = call_layout::plan(
            &tiles,
            self.size,
            &self.layout,
            self.speaker.current(),
            if compact { 0 } else { self.page },
            Proportions { strip: self.strip, self_view: self.self_view },
            !self.video(),
        );
        if compact && let Some(main) = plan.tiles.first() {
            let page = self.page.min(tiles.len().saturating_sub(1));
            let selected = if page == 0 {
                tiles.iter().find(|tile| tile.key == main.key)
            } else {
                tiles.iter().filter(|tile| tile.key != main.key).nth(page - 1)
            };
            if let Some(tile) = selected {
                let mut compact = CallPlan::single(tile.key.clone(), self.size, tile.aspect);
                compact.page = page;
                compact.pages = tiles.len();
                return compact;
            }
        }
        plan
    }

    /// The one-to-one self-view's rectangle and the remote video's it floats over, while the
    /// stage shows one.
    pub fn self_view_rects(&self) -> Option<([f32; 4], [f32; 4])> {
        let plan = self.plan();
        let own = plan.tiles.iter().find(|tile| tile.inset)?;
        let main = plan.tiles.iter().find(|tile| !tile.inset)?;
        Some((own.rect, main.rect))
    }

    /// Follows the user dragging the self-view `offset` from its corner, keeping it over the
    /// remote video.
    pub fn drag_self_view(&mut self, offset: [f32; 2]) {
        let Some((own, main)) = self.self_view_rects() else { return };
        let within = |value: f32, axis: usize| {
            let (low, high) = (main[axis] - own[axis], main[axis] + main[axis + 2] - own[axis] - own[axis + 2]);
            value.clamp(low.min(0.0), high.max(0.0))
        };
        self.self_drag = Some([within(offset[0], 0), within(offset[1], 1)]);
    }

    /// The user let go of the self-view: it snaps to the remote video's corner nearest where
    /// it was dropped. A cancelled drag (`None`) puts it back.
    pub fn drop_self_view(&mut self, dropped: Option<[f32; 2]>) {
        if let (Some(offset), Some((own, main))) = (dropped, self.self_view_rects()) {
            let centre = [own[0] + own[2] / 2.0 + offset[0], own[1] + own[3] / 2.0 + offset[1]];
            self.self_view.corner = Corner::nearest(main, centre);
        }
        self.self_drag = None;
    }

    /// Makes the self-view `scale` of the remote video's width, within its limits.
    pub fn resize_self_view(&mut self, scale: f32) {
        self.self_view.scale = SelfView::clamp(scale);
    }

    pub fn chat_beside(&self) -> bool {
        self.room_size[0] >= 780.0
    }

    pub fn compact(&self) -> bool {
        self.chat && !self.chat_beside()
    }

    pub fn visible_videos(&self) -> HashSet<VideoId> {
        self.plan().tiles.iter().filter_map(|tile| self.video_id(&tile.key)).collect()
    }

    /// A fingerprint of everything [`Call::visible_videos`] reads: the videos, their shapes, the
    /// layout and its sizes, the page and who speaks. Cheap enough for every frame, where the
    /// plan itself isn't.
    pub fn videos_key(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        let info = &self.info;
        (info.video, info.camera, info.screen).hash(&mut hasher);
        let own = [info.camera, info.screen];
        for participant in &info.participants {
            (&participant.key, participant.camera, participant.screen).hash(&mut hasher);
        }
        let videos = own.into_iter().chain(info.participants.iter().flat_map(|p| [p.camera, p.screen])).flatten();
        for video in videos {
            self.aspects.get(&video).map(|aspect| aspect.to_bits()).hash(&mut hasher);
        }
        let floats = [self.size[0], self.size[1], self.room_size[0], self.room_size[1], self.strip];
        floats.map(f32::to_bits).hash(&mut hasher);
        (self.show_self, self.chat, &self.layout, self.speaker.current(), self.page).hash(&mut hasher);
        hasher.finish()
    }

    /// Frames still go straight to Pixels. Only a change of source geometry rebuilds the view.
    pub fn measure_videos(&mut self) -> bool {
        let mut changed = false;
        for (id, pixels) in &self.videos {
            let (width, height) = pixels.size();
            if width == 0 || height == 0 {
                continue;
            }
            let aspect = width as f32 / height as f32;
            if self.aspects.get(id).is_none_or(|old| (old - aspect).abs() > 0.001) {
                self.aspects.insert(*id, aspect);
                changed = true;
            }
        }
        changed
    }

    /// The levels the glow follows: the user first, then everyone in the call's order.
    pub fn glow_levels(&self) -> Vec<f32> {
        let own = self.levels.get("").copied().unwrap_or(0.0);
        std::iter::once(own)
            .chain(self.info.participants.iter().map(|p| self.levels.get(&p.key).copied().unwrap_or(0.0)))
            .collect()
    }
}

impl State {
    pub fn open_call_layout(&mut self) {
        self.toggle_popover(super::Popover::CallLayout);
    }

    pub fn toggle_call_sidebar(&mut self) {
        if let Some(call) = self.call.as_mut() {
            call.hide_sidebar = !call.hide_sidebar;
        }
    }

    pub fn set_call_layout(&mut self, layout: CallLayout) {
        self.popover = None;
        if let Some(call) = self.call.as_mut() {
            call.layout = layout;
            call.page = 0;
        }
        self.sync_watching();
    }

    pub fn resize_call(&mut self, size: [f32; 2]) -> bool {
        let Some(call) = self.call.as_mut() else { return false };
        if call.size.iter().zip(size).all(|(old, new)| (old - new).abs() < 0.5) {
            return false;
        }
        call.size = size;
        call.page = call.plan().page;
        self.sync_watching();
        true
    }

    pub fn page_call(&mut self, forward: bool) {
        if let Some(call) = self.call.as_mut() {
            let plan = call.plan();
            call.page =
                if forward { (plan.page + 1).min(plan.pages.saturating_sub(1)) } else { plan.page.saturating_sub(1) };
        }
        self.sync_watching();
    }

    pub fn toggle_call_self(&mut self) {
        if let Some(call) = self.call.as_mut() {
            call.show_self = !call.show_self;
            if !call.show_self
                && matches!(&call.layout, CallLayout::Focus(Some(CallTile::Camera(key))) if key.is_empty())
            {
                call.layout = CallLayout::Auto;
            }
            call.page = 0;
        }
        self.sync_watching();
    }

    pub fn toggle_call_fit(&mut self, key: CallTile) {
        if let Some(call) = self.call.as_mut() {
            let full = call.full_video.get(&key).copied().unwrap_or_else(|| call.aspect(&key) < 1.0);
            call.full_video.insert(key, !full);
        }
    }
}

#[cfg(test)]
mod tests;
