use super::*;
use std::io::Read;

fn random(seed: &mut u32) -> u8 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    *seed as u8
}

fn round_trip(pixels: &[u8], width: u32) {
    let encoded = compress(pixels, width).unwrap();
    let mut decoded = Vec::new();
    flate2::read::ZlibDecoder::new(encoded.as_slice())
        .read_to_end(&mut decoded)
        .unwrap();
    assert_eq!(decoded, pixels);
}

#[test]
fn noisy_full_images_use_stored_blocks_and_still_round_trip_every_pixel() {
    let mut seed = 19;
    let pixels = (0..512 * 256 * 3)
        .map(|_| random(&mut seed))
        .collect::<Vec<_>>();
    assert_eq!(choose(&pixels, 512).unwrap().level(), 0);
    round_trip(&pixels, 512);
}

#[test]
fn contiguous_sampling_preserves_compression_for_correlated_random_scanlines() {
    let mut seed = 23;
    let row = (0..2560 * 3).map(|_| random(&mut seed)).collect::<Vec<_>>();
    let pixels = row.repeat(64);
    assert_eq!(choose(&pixels, 2560).unwrap(), Compression::fast());
    let encoded = compress(&pixels, 2560).unwrap();
    assert!(encoded.len() < pixels.len() / 4);
    round_trip(&pixels, 2560);
}

#[test]
fn text_like_images_and_small_rectangles_keep_fast_compression() {
    let pixels = [235, 237, 240].repeat(512 * 256);
    assert_eq!(choose(&pixels, 512).unwrap(), Compression::fast());
    round_trip(&pixels, 512);
    let mut seed = 31;
    let patch = (0..128 * 128 * 3)
        .map(|_| random(&mut seed))
        .collect::<Vec<_>>();
    assert_eq!(choose(&patch, 128).unwrap(), Compression::fast());
    round_trip(&patch, 128);
}

#[test]
fn mixed_entropy_fast_compression_does_not_grow_its_output_allocation() {
    let mut seed = 31;
    let pixels: Vec<_> = (0..512 * 256 * 3)
        .map(|index| {
            if index % (512 * 3) < 256 * 3 {
                random(&mut seed)
            } else {
                235
            }
        })
        .collect();
    assert_eq!(choose(&pixels, 512).unwrap(), Compression::fast());
    let encoded = compress(&pixels, 512).unwrap();
    assert!(encoded.len() < pixels.len() * 3 / 4);
    assert_eq!(encoded.capacity(), pixels.len() + pixels.len() / 1000 + 128);
    round_trip(&pixels, 512);
}
