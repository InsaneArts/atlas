use super::*;

/// `height` rows `stride` bytes long, all of one grey level.
fn picture(height: usize, stride: usize, grey: u8) -> Vec<u8> {
    vec![grey; stride * height]
}

fn pixels(rgb: &[u8], width: usize, height: usize, stride: usize) -> Pixels<'_> {
    Pixels {
        rgb,
        width,
        height,
        stride,
    }
}

/// An encoder making `bytes` bytes for every picture, counting its calls.
fn sized(
    bytes: usize,
    calls: &mut Vec<(usize, usize, u32, f64)>,
) -> impl FnMut(Pixels<'_>, (u32, f64)) -> Result<Vec<u8>> + '_ {
    move |picture, (limit, quality)| {
        calls.push((picture.width, picture.height, limit, quality));
        Ok(vec![0; bytes])
    }
}

#[test]
fn the_tiles_cover_the_picture_exactly_edges_included() {
    let covered = rects((600, 300));
    assert_eq!(
        covered,
        vec![
            (0, 0, 256, 256),
            (256, 0, 256, 256),
            (512, 0, 88, 256),
            (0, 256, 256, 44),
            (256, 256, 256, 44),
            (512, 256, 88, 44),
        ]
    );
    let area: usize = covered.iter().map(|(_, _, w, h)| w * h).sum();
    assert_eq!(area, 600 * 300);
}

#[test]
fn a_view_with_nothing_gets_a_base_and_every_tile_that_fits() -> Result<()> {
    let rgb = picture(300, 1800, 7);
    let mut calls = Vec::new();
    let tiled = Tiles::default().next_with(
        None,
        pixels(&rgb, 600, 300, 1800),
        &mut sized(100, &mut calls),
    )?;
    assert!(tiled.base.is_some());
    assert_eq!(tiled.tiles.len(), 6);
    assert!(!tiled.more);
    assert_eq!((tiled.width, tiled.height), (600, 300));
    // The base first, at the small frame's size; then each tile at its own.
    assert_eq!(calls[0], (600, 300, BASE.0, BASE.1));
    assert_eq!(calls[3], (88, 256, 256, QUALITY));
    Ok(())
}

#[test]
fn an_unchanged_screen_sends_nothing_after_its_picture() -> Result<()> {
    let rgb = picture(300, 1800, 7);
    let mut tiles = Tiles::default();
    let mut calls = Vec::new();
    let first = tiles.next_with(
        None,
        pixels(&rgb, 600, 300, 1800),
        &mut sized(100, &mut calls),
    )?;
    let again = tiles.next_with(
        Some(&first.id),
        pixels(&rgb, 600, 300, 1800),
        &mut sized(100, &mut calls),
    )?;
    assert!(again.unchanged());
    assert_eq!(again.id, first.id);
    Ok(())
}

#[test]
fn only_the_tiles_that_changed_are_sent() -> Result<()> {
    let mut rgb = picture(300, 1800, 7);
    let mut tiles = Tiles::default();
    let mut calls = Vec::new();
    let first = tiles.next_with(
        None,
        pixels(&rgb, 600, 300, 1800),
        &mut sized(100, &mut calls),
    )?;
    // One pixel in the bottom right tile.
    rgb[299 * 1800 + 599 * 3] = 200;
    let next = tiles.next_with(
        Some(&first.id),
        pixels(&rgb, 600, 300, 1800),
        &mut sized(100, &mut calls),
    )?;
    assert!(next.base.is_none());
    assert_eq!(next.tiles.len(), 1);
    assert_eq!((next.tiles[0].x, next.tiles[0].y), (512, 256));
    assert_ne!(next.id, first.id);
    Ok(())
}

#[test]
fn padding_past_a_row_is_not_part_of_the_picture() -> Result<()> {
    let mut rgb = picture(300, 1856, 7);
    let mut tiles = Tiles::default();
    let mut calls = Vec::new();
    let first = tiles.next_with(
        None,
        pixels(&rgb, 600, 300, 1856),
        &mut sized(100, &mut calls),
    )?;
    rgb[1800] = 99;
    let next = tiles.next_with(
        Some(&first.id),
        pixels(&rgb, 600, 300, 1856),
        &mut sized(100, &mut calls),
    )?;
    assert!(next.unchanged());
    Ok(())
}

#[test]
fn an_unknown_picture_or_another_size_starts_again() -> Result<()> {
    let rgb = picture(300, 1800, 7);
    let mut tiles = Tiles::default();
    let mut calls = Vec::new();
    let first = tiles.next_with(
        None,
        pixels(&rgb, 600, 300, 1800),
        &mut sized(100, &mut calls),
    )?;
    let unknown = tiles.next_with(
        Some("0123456789abcdef"),
        pixels(&rgb, 600, 300, 1800),
        &mut sized(100, &mut calls),
    )?;
    assert!(unknown.base.is_some());
    assert_eq!(unknown.tiles.len(), 6);
    let smaller = picture(150, 900, 7);
    let resized = tiles.next_with(
        Some(&first.id),
        pixels(&smaller, 300, 150, 900),
        &mut sized(100, &mut calls),
    )?;
    assert!(resized.base.is_some());
    Ok(())
}

#[test]
fn tiles_past_the_budget_wait_for_the_next_answer() -> Result<()> {
    let rgb = picture(512, 3072, 7);
    let mut tiles = Tiles::default();
    let mut calls = Vec::new();
    // Each tile takes about a third of an answer.
    let third = FRAME_BUDGET / 4;
    let first = tiles.next_with(
        None,
        pixels(&rgb, 1024, 512, 3072),
        &mut sized(third, &mut calls),
    )?;
    assert!(first.more);
    let sent = first.tiles.len();
    assert!(sent < 8);
    let mut total = sent;
    let mut since = first.id;
    loop {
        let next = tiles.next_with(
            Some(&since),
            pixels(&rgb, 1024, 512, 3072),
            &mut sized(third, &mut calls),
        )?;
        assert!(next.base.is_none());
        let used: usize = next
            .tiles
            .iter()
            .map(|tile| tile.data.len() + TILE_OVERHEAD)
            .sum();
        assert!(used <= FRAME_BUDGET);
        total += next.tiles.len();
        since = next.id;
        if !next.more {
            break;
        }
    }
    assert_eq!(total, 8);
    Ok(())
}

/// An encoder whose JPEG of a whole picture or of a tile takes `whole` or
/// `tile` bytes at the base and tile qualities, and `small` bytes at the
/// lower ones.
fn shaped(
    whole: usize,
    tile: usize,
    small: usize,
) -> impl FnMut(Pixels<'_>, (u32, f64)) -> Result<Vec<u8>> {
    move |picture, (_, quality)| {
        let bytes = if picture.width > TILE || picture.height > TILE {
            if quality == BASE.1 { whole } else { small }
        } else if quality == QUALITY {
            tile
        } else {
            small
        };
        Ok(vec![0; bytes])
    }
}

/// The bytes one tile costs, so `n` of them fill an answer.
fn share(n: usize) -> usize {
    (FRAME_BUDGET / n - TILE_OVERHEAD) / 4 * 3
}

#[test]
fn changes_too_large_for_one_answer_come_as_a_base_so_the_view_shows_one_screen() -> Result<()> {
    let rgb = picture(512, 3072, 7);
    let mut tiles = Tiles::default();
    let mut first = tiles.next_with(
        None,
        pixels(&rgb, 1024, 512, 3072),
        &mut shaped(100, 100, 50),
    )?;
    while first.more {
        let id = first.id.clone();
        first = tiles.next_with(
            Some(&id),
            pixels(&rgb, 1024, 512, 3072),
            &mut shaped(100, 100, 50),
        )?;
    }
    // Every tile changes, and each takes a third of an answer.
    let changed = picture(512, 3072, 9);
    let next = tiles.next_with(
        Some(&first.id),
        pixels(&changed, 1024, 512, 3072),
        &mut shaped(100, share(3), 50),
    )?;
    assert!(next.base.is_some(), "the whole new screen, small");
    assert!(next.more, "sharper tiles follow");
    let used: usize = next.base.as_ref().map_or(0, String::len)
        + next
            .tiles
            .iter()
            .map(|tile| tile.data.len() + TILE_OVERHEAD)
            .sum::<usize>();
    assert!(used <= FRAME_BUDGET);
    Ok(())
}

#[test]
fn changes_that_fit_come_sharp_with_no_base() -> Result<()> {
    let rgb = picture(512, 3072, 7);
    let mut tiles = Tiles::default();
    let mut shown = tiles.next_with(
        None,
        pixels(&rgb, 1024, 512, 3072),
        &mut shaped(100, 100, 50),
    )?;
    while shown.more {
        let id = shown.id.clone();
        shown = tiles.next_with(
            Some(&id),
            pixels(&rgb, 1024, 512, 3072),
            &mut shaped(100, 100, 50),
        )?;
    }
    let mut changed = rgb.clone();
    changed[0] = 1;
    changed[3072 * 300 + 3 * 900] = 1;
    let next = tiles.next_with(
        Some(&shown.id),
        pixels(&changed, 1024, 512, 3072),
        &mut shaped(100, share(3), 50),
    )?;
    assert!(next.base.is_none());
    assert_eq!(next.tiles.len(), 2);
    assert!(!next.more);
    Ok(())
}

#[test]
fn a_changing_corner_never_keeps_the_rest_from_sharpening() -> Result<()> {
    let mut rgb = picture(512, 3072, 7);
    let mut tiles = Tiles::default();
    // Each tile takes a third of an answer: the first answer is a base and
    // two tiles, and every later one a changed corner and one more tile.
    let mut answer = tiles.next_with(
        None,
        pixels(&rgb, 1024, 512, 3072),
        &mut shaped(100, share(3), 50),
    )?;
    for round in 0..8 {
        if !answer.more {
            break;
        }
        rgb[0] = round;
        let id = answer.id.clone();
        answer = tiles.next_with(
            Some(&id),
            pixels(&rgb, 1024, 512, 3072),
            &mut shaped(100, share(3), 50),
        )?;
        assert!(answer.base.is_none(), "the corner fits, so no base again");
    }
    assert!(
        !answer.more,
        "every tile came sharp while the corner kept changing"
    );
    Ok(())
}

#[test]
fn a_tile_too_detailed_for_an_answer_comes_smaller_or_stays_as_the_base_drew_it() -> Result<()> {
    let rgb = picture(256, 768, 7);
    let mut tiles = Tiles::default();
    let budget = FRAME_BUDGET / 4 * 3;
    // At the tile quality it never fits; at the lower one it comes at once.
    let first = tiles.next_with(
        None,
        pixels(&rgb, 256, 256, 768),
        &mut shaped(100, budget, 10),
    )?;
    assert!(first.base.is_some());
    assert_eq!(first.tiles.len(), 1);
    assert!(!first.more);
    // Never fitting at all, it stays as the base drew it and is not asked for again.
    let mut never = Tiles::default();
    let first = never.next_with(
        None,
        pixels(&rgb, 256, 256, 768),
        &mut shaped(100, budget, budget),
    )?;
    let next = never.next_with(
        Some(&first.id),
        pixels(&rgb, 256, 256, 768),
        &mut shaped(100, budget, budget),
    )?;
    assert!(next.tiles.is_empty());
    assert!(!next.more);
    Ok(())
}

#[test]
fn a_base_too_detailed_is_sent_smaller_or_refused() -> Result<()> {
    let rgb = picture(512, 3072, 7);
    let budget = FRAME_BUDGET / 4 * 3;
    let smaller = Tiles::default().next_with(
        None,
        pixels(&rgb, 1024, 512, 3072),
        &mut shaped(budget + 3, 100, 100),
    )?;
    assert_eq!(
        smaller.base.map(|base| base.len()),
        Some(base64_length(100))
    );
    let refused = Tiles::default().next_with(
        None,
        pixels(&rgb, 1024, 512, 3072),
        &mut shaped(budget + 3, 100, budget + 3),
    );
    assert!(refused.is_err());
    Ok(())
}

#[test]
fn pixels_short_of_their_size_are_refused() {
    let rgb = picture(10, 30, 7);
    let mut calls = Vec::new();
    let short = Tiles::default().next_with(
        None,
        pixels(&rgb[..200], 10, 10, 30),
        &mut sized(1, &mut calls),
    );
    assert!(short.is_err());
    let narrow =
        Tiles::default().next_with(None, pixels(&rgb, 10, 10, 20), &mut sized(1, &mut calls));
    assert!(narrow.is_err());
}

#[test]
fn a_tile_behind_changes_that_fill_every_answer_stops_waiting() -> Result<()> {
    let mut rgb = picture(256, 1536, 7);
    let mut tiles = Tiles::default();
    // Two tiles: the left one changes every time and fills an answer, the
    // right one waits to sharpen and never fits beside it.
    let mut encode = |picture: Pixels<'_>, (_, quality): (u32, f64)| -> Result<Vec<u8>> {
        let bytes = if picture.width > TILE {
            100
        } else if picture.rgb[0] != 7 {
            share(1) - 100
        } else if quality == QUALITY {
            share(2)
        } else {
            share(3)
        };
        Ok(vec![0; bytes])
    };
    let mut answer = tiles.next_with(None, pixels(&rgb, 512, 256, 1536), &mut encode)?;
    let mut rounds = 0;
    while answer.more {
        rounds += 1;
        assert!(rounds <= usize::from(MOST_WAITS) + 1, "it stops asking");
        rgb[0] = 8 + (rounds % 2) as u8;
        let id = answer.id.clone();
        answer = tiles.next_with(Some(&id), pixels(&rgb, 512, 256, 1536), &mut encode)?;
        assert!(answer.base.is_none());
    }
    Ok(())
}
