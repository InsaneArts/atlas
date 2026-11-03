use super::*;

#[test]
fn this_process_reads_its_own_footprint_and_peak() {
    let usage = own().unwrap();
    assert!(usage.footprint > 0);
    assert!(usage.resident > 0);
    assert!(usage.peak_footprint >= usage.footprint);
    let value = serde_json::to_value(usage).unwrap();
    assert!(value["peakFootprint"].is_u64());
}
