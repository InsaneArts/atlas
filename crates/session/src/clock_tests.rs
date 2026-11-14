use super::*;

#[test]
fn the_continuous_clock_moves_forward() {
    let before = Continuous.now().unwrap();
    std::thread::sleep(Duration::from_millis(5));

    let after = Continuous.now().unwrap();

    assert!(after > before);
}
