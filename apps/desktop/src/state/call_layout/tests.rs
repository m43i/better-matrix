use super::*;

fn cameras(count: usize) -> Vec<TileGeometry> {
    (0..count).map(|i| TileGeometry { key: CallTile::Camera(i.to_string()), aspect: ASPECT }).collect()
}

#[test]
fn a_grid_should_use_the_available_rectangle() {
    let tiles = cameras(6);
    let wide = plan(&tiles, [1000.0, 300.0], &CallLayout::Grid, None, 0, 0.24, false);
    let tall = plan(&tiles, [400.0, 900.0], &CallLayout::Grid, None, 0, 0.24, false);
    assert!(wide.tiles[0].rect[1] == wide.tiles[1].rect[1] && tall.tiles[0].rect[1] != tall.tiles[1].rect[1]);
}

#[test]
fn an_incomplete_row_should_keep_the_other_tiles_size() {
    let result = plan(&cameras(5), [1000.0, 500.0], &CallLayout::Grid, None, 0, 0.24, false);
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
                let result = plan(&tiles, size, &layout, None, 100, 0.24, false);
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
    let result = plan(&cameras(40), [800.0, 450.0], &CallLayout::Grid, None, 0, 0.24, false);
    assert!(
        result.pages > 1 && result.tiles.iter().all(|tile| tile.rect[2] >= MIN_WIDTH && tile.rect[3] >= MIN_HEIGHT)
    );
}

#[test]
fn an_out_of_range_page_should_show_the_last_page() {
    let result = plan(&cameras(40), [800.0, 450.0], &CallLayout::Grid, None, usize::MAX, 0.24, false);
    assert_eq!(result.page, result.pages - 1);
}

#[test]
fn a_shared_screen_should_keep_its_native_aspect() {
    let tiles = [TileGeometry { key: CallTile::Screen("a".into()), aspect: 9.0 / 16.0 }];
    let result = plan(&tiles, [1000.0, 500.0], &CallLayout::Auto, None, 0, 0.24, false);
    assert!((result.tiles[0].rect[2] / result.tiles[0].rect[3] - 9.0 / 16.0).abs() < 0.001);
}

#[test]
fn a_narrow_share_view_should_put_the_participants_below() {
    let mut tiles = cameras(5);
    tiles.push(TileGeometry { key: CallTile::Screen("a".into()), aspect: ASPECT });
    let result = plan(&tiles, [400.0, 500.0], &CallLayout::Auto, None, 0, 0.24, false);
    assert!(result.divider.is_some() && !result.strip_beside);
}

#[test]
fn a_wide_share_view_should_put_the_participants_beside_it() {
    let mut tiles = cameras(5);
    tiles.push(TileGeometry { key: CallTile::Screen("a".into()), aspect: ASPECT });
    let result = plan(&tiles, [1000.0, 500.0], &CallLayout::Auto, None, 0, 0.24, false);
    assert!(result.divider.is_some() && result.strip_beside);
}

#[test]
fn a_grid_should_allow_shared_screens_to_be_compared() {
    let tiles = ["a", "b"].map(|key| TileGeometry { key: CallTile::Screen(key.into()), aspect: ASPECT });
    let result = plan(&tiles, [1000.0, 500.0], &CallLayout::Grid, None, 0, 0.24, false);
    assert_eq!(result.tiles.len(), 2);
}

#[test]
fn a_pin_should_override_the_speaker() {
    let key = CallTile::Camera("2".into());
    let result = plan(&cameras(5), [1000.0, 500.0], &CallLayout::Focus(Some(key.clone())), Some("1"), 0, 0.24, false);
    assert_eq!(result.tiles[0].key, key);
}

#[test]
fn a_direct_call_should_keep_a_small_self_view() {
    let tiles = [
        TileGeometry { key: CallTile::Camera("remote".into()), aspect: ASPECT },
        TileGeometry { key: CallTile::Camera(String::new()), aspect: ASPECT },
    ];
    let result = plan(&tiles, [800.0, 500.0], &CallLayout::Auto, None, 0, 0.24, false);
    assert!(result.tiles[1].inset && result.tiles[1].rect[2] < result.tiles[0].rect[2] / 2.0);
}

#[test]
fn a_compact_voice_call_should_fit_several_faces() {
    let result = plan(&cameras(4), [360.0, 100.0], &CallLayout::Auto, None, 0, 0.24, true);
    assert!(result.tiles.len() >= 2);
}

#[test]
fn hiding_self_view_should_keep_two_remote_participants_in_a_gallery() {
    let result = plan(&cameras(2), [800.0, 450.0], &CallLayout::Auto, None, 0, 0.24, false);
    assert!(result.divider.is_none() && result.tiles[0].rect[2..] == result.tiles[1].rect[2..]);
}
