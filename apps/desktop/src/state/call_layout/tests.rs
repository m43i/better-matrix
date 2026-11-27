use super::*;

fn cameras(count: usize) -> Vec<TileGeometry> {
    (0..count).map(|i| TileGeometry { key: CallTile::Camera(i.to_string()), aspect: ASPECT }).collect()
}

#[test]
fn a_grid_should_use_the_available_rectangle() {
    let tiles = cameras(6);
    let wide = plan(&tiles, [1000.0, 300.0], &CallLayout::Grid, None, 0, Proportions::default(), false);
    let tall = plan(&tiles, [400.0, 900.0], &CallLayout::Grid, None, 0, Proportions::default(), false);
    assert!(wide.tiles[0].rect[1] == wide.tiles[1].rect[1] && tall.tiles[0].rect[1] != tall.tiles[1].rect[1]);
}

#[test]
fn an_incomplete_row_should_keep_the_other_tiles_size() {
    let result = plan(&cameras(5), [1000.0, 500.0], &CallLayout::Grid, None, 0, Proportions::default(), false);
    let first = &result.tiles[0].rect;
    assert!(result.tiles.iter().all(|tile| tile.rect[2..] == first[2..]));
}

#[test]
fn layouts_should_keep_non_inset_tiles_inside_the_viewport_and_apart() {
    for size in [[1.0, 1.0], [200.0, 120.0], [360.0, 400.0], [1000.0, 300.0], [1800.0, 900.0]] {
        for count in [0, 1, 2, 3, 5, 9, 50] {
            let mut tiles = cameras(count);
            tiles.push(TileGeometry { key: CallTile::Screen("share".into()), aspect: 9.0 / 16.0 });
            for layout in [CallLayout::Auto, CallLayout::Grid, CallLayout::Focus(None)] {
                let result = plan(&tiles, size, &layout, None, 100, Proportions::default(), false);
                for (i, tile) in result.tiles.iter().enumerate() {
                    let [x, y, w, h] = tile.rect;
                    assert!(
                        x >= -0.01
                            && y >= -0.01
                            && w >= 0.0
                            && h >= 0.0
                            && x + w <= size[0] + 0.01
                            && y + h <= size[1] + 0.01,
                        "{size:?} {count} {layout:?}: {:?}",
                        tile.rect
                    );
                    for other in &result.tiles[i + 1..] {
                        let [ox, oy, ow, oh] = other.rect;
                        assert!(
                            tile.inset
                                || other.inset
                                || w == 0.0
                                || h == 0.0
                                || ow == 0.0
                                || oh == 0.0
                                || x + w <= ox + 0.01
                                || ox + ow <= x + 0.01
                                || y + h <= oy + 0.01
                                || oy + oh <= y + 0.01
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn a_crowded_call_should_page_before_tiles_become_unreadable() {
    let result = plan(&cameras(40), [800.0, 450.0], &CallLayout::Grid, None, 0, Proportions::default(), false);
    assert!(
        result.pages > 1 && result.tiles.iter().all(|tile| tile.rect[2] >= MIN_WIDTH && tile.rect[3] >= MIN_HEIGHT)
    );
}

#[test]
fn an_out_of_range_page_should_show_the_last_page() {
    let result = plan(&cameras(40), [800.0, 450.0], &CallLayout::Grid, None, usize::MAX, Proportions::default(), false);
    assert_eq!(result.page, result.pages - 1);
}

#[test]
fn a_shared_screen_should_keep_its_native_aspect() {
    let tiles = [TileGeometry { key: CallTile::Screen("a".into()), aspect: 9.0 / 16.0 }];
    let result = plan(&tiles, [1000.0, 500.0], &CallLayout::Auto, None, 0, Proportions::default(), false);
    assert!((result.tiles[0].rect[2] / result.tiles[0].rect[3] - 9.0 / 16.0).abs() < 0.001);
}

#[test]
fn a_narrow_share_view_should_put_the_participants_below() {
    let mut tiles = cameras(5);
    tiles.push(TileGeometry { key: CallTile::Screen("a".into()), aspect: ASPECT });
    let result = plan(&tiles, [400.0, 500.0], &CallLayout::Auto, None, 0, Proportions::default(), false);
    assert!(result.divider.is_some() && !result.strip_beside);
}

#[test]
fn a_wide_share_view_should_put_the_participants_beside_it() {
    let mut tiles = cameras(5);
    tiles.push(TileGeometry { key: CallTile::Screen("a".into()), aspect: ASPECT });
    let result = plan(&tiles, [1000.0, 500.0], &CallLayout::Auto, None, 0, Proportions::default(), false);
    assert!(result.divider.is_some() && result.strip_beside);
}

#[test]
fn a_grid_should_allow_shared_screens_to_be_compared() {
    let tiles = ["a", "b"].map(|key| TileGeometry { key: CallTile::Screen(key.into()), aspect: ASPECT });
    let result = plan(&tiles, [1000.0, 500.0], &CallLayout::Grid, None, 0, Proportions::default(), false);
    assert_eq!(result.tiles.len(), 2);
}

#[test]
fn a_pin_should_override_the_speaker() {
    let key = CallTile::Camera("2".into());
    let result = plan(
        &cameras(5),
        [1000.0, 500.0],
        &CallLayout::Focus(Some(key.clone())),
        Some("1"),
        0,
        Proportions::default(),
        false,
    );
    assert_eq!(result.tiles[0].key, key);
}

#[test]
fn a_direct_call_should_keep_a_small_self_view() {
    let tiles = [
        TileGeometry { key: CallTile::Camera("remote".into()), aspect: ASPECT },
        TileGeometry { key: CallTile::Camera(String::new()), aspect: ASPECT },
    ];
    let result = plan(&tiles, [800.0, 500.0], &CallLayout::Auto, None, 0, Proportions::default(), false);
    assert!(result.tiles[1].inset && result.tiles[1].rect[2] < result.tiles[0].rect[2] / 2.0);
}

fn direct(remote_aspect: f32, own_aspect: f32, size: [f32; 2], self_view: SelfView) -> CallPlan {
    let tiles = [
        TileGeometry { key: CallTile::Camera("remote".into()), aspect: remote_aspect },
        TileGeometry { key: CallTile::Camera(String::new()), aspect: own_aspect },
    ];
    let proportions = Proportions { self_view, ..Proportions::default() };
    plan(&tiles, size, &CallLayout::Auto, None, 0, proportions, false)
}

fn inside(inner: [f32; 4], outer: [f32; 4]) -> bool {
    inner[0] >= outer[0] - 0.01
        && inner[1] >= outer[1] - 0.01
        && inner[0] + inner[2] <= outer[0] + outer[2] + 0.01
        && inner[1] + inner[3] <= outer[1] + outer[3] + 0.01
}

#[test]
fn a_self_view_should_sit_in_the_corner_of_the_remote_video_not_the_viewport() {
    // A 4:3 remote video in a wide viewport leaves bars beside it.
    let result = direct(4.0 / 3.0, ASPECT, [1200.0, 500.0], SelfView::default());
    let (remote, own) = (result.tiles[0].rect, result.tiles[1].rect);
    assert!(remote[0] > 100.0 && inside(own, remote), "{remote:?} {own:?}");
    assert!((remote[0] + remote[2] - (own[0] + own[2]) - GAP).abs() < 0.01);
    assert!((remote[1] + remote[3] - (own[1] + own[3]) - GAP).abs() < 0.01);
}

#[test]
fn a_self_view_should_go_to_the_corner_the_user_chose() {
    for corner in [Corner::TopLeft, Corner::TopRight, Corner::BottomLeft, Corner::BottomRight] {
        let result = direct(ASPECT, ASPECT, [800.0, 450.0], SelfView { corner, ..SelfView::default() });
        let (remote, own) = (result.tiles[0].rect, result.tiles[1].rect);
        let centre = [own[0] + own[2] / 2.0, own[1] + own[3] / 2.0];
        assert_eq!(Corner::nearest(remote, centre), corner);
    }
}

#[test]
fn a_self_view_should_keep_its_cameras_shape() {
    let result = direct(ASPECT, 9.0 / 16.0, [800.0, 450.0], SelfView::default());
    let own = result.tiles[1].rect;
    assert!((own[2] / own[3] - 9.0 / 16.0).abs() < 0.001, "{own:?}");
}

#[test]
fn a_self_view_should_grow_and_shrink_only_within_its_limits() {
    let size = [1000.0, 562.5];
    let width = |scale| direct(ASPECT, ASPECT, size, SelfView { scale, ..SelfView::default() }).tiles[1].rect[2];
    assert!(width(0.4) > width(0.25));
    assert!((width(5.0) - width(*SELF_VIEW_SCALE.end())).abs() < 0.01);
    assert!((width(0.0) - width(*SELF_VIEW_SCALE.start())).abs() < 0.01);
    assert!(width(5.0) <= size[0] * 0.5);
}

#[test]
fn a_portrait_self_view_over_a_short_video_should_still_resize() {
    let size = [600.0, 337.5];
    let width = |scale| direct(ASPECT, 9.0 / 16.0, size, SelfView { scale, ..SelfView::default() }).tiles[1].rect[2];
    assert!(width(0.0) < width(5.0), "{} {}", width(0.0), width(5.0));
}

#[test]
fn a_self_view_should_stay_inside_a_tiny_remote_video() {
    for size in [[1.0, 1.0], [20.0, 20.0], [120.0, 60.0]] {
        for corner in [Corner::TopLeft, Corner::BottomRight] {
            let result = direct(ASPECT, ASPECT, size, SelfView { corner, scale: 0.45 });
            assert!(inside(result.tiles[1].rect, result.tiles[0].rect), "{size:?} {:?}", result.tiles);
        }
    }
}

#[test]
fn a_dropped_self_view_should_snap_to_the_nearest_corner() {
    let video = [100.0, 50.0, 800.0, 400.0];
    assert_eq!(Corner::nearest(video, [150.0, 60.0]), Corner::TopLeft);
    assert_eq!(Corner::nearest(video, [880.0, 100.0]), Corner::TopRight);
    assert_eq!(Corner::nearest(video, [480.0, 300.0]), Corner::BottomLeft);
    // Dragged past the video's edge, it still snaps to that side's corner.
    assert_eq!(Corner::nearest(video, [2000.0, 2000.0]), Corner::BottomRight);
}

#[test]
fn a_compact_voice_call_should_fit_several_faces() {
    let result = plan(&cameras(4), [360.0, 100.0], &CallLayout::Auto, None, 0, Proportions::default(), true);
    assert!(result.tiles.len() >= 2);
}

#[test]
fn hiding_self_view_should_keep_two_remote_participants_in_a_gallery() {
    let result = plan(&cameras(2), [800.0, 450.0], &CallLayout::Auto, None, 0, Proportions::default(), false);
    assert!(result.divider.is_none() && result.tiles[0].rect[2..] == result.tiles[1].rect[2..]);
}
