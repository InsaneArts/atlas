use super::*;

fn failed(id: u64, stage: Stage, reason: Reason) -> Record {
    let mut record = Record::new(id);
    record.end(Outcome::Rejected, stage, reason);
    record
}

#[test]
fn a_record_serializes_only_fixed_words_and_numbers() {
    let mut record = failed(7, Stage::AfterProof, Reason::SourceStale);
    record.requested_backend = Backend::Sck;
    record.actual_backend = Backend::CgFallback;
    record.time(Timing::BeforeProof, Duration::from_millis(3));
    record.time(Timing::Total, Duration::from_millis(520));
    let value = serde_json::to_value(record).unwrap();
    assert_eq!(value["version"], 1);
    assert_eq!(value["stage"], "after-proof");
    assert_eq!(value["reason"], "source-stale");
    assert_eq!(value["actualBackend"], "cg-fallback");
    assert_eq!(value["timingMask"], 0b1000_0001);
    assert_eq!(value["timingsNs"][0], 3_000_000);
    assert_eq!(value["timingsNs"][7], 520_000_000);
    assert!(
        value
            .as_object()
            .unwrap()
            .values()
            .all(|field| !field.is_string() || field.as_str().unwrap().len() < 32),
        "only short fixed words"
    );
    assert!(serde_json::to_vec(&record).unwrap().len() <= 2048);
    assert!(std::mem::size_of::<Record>() <= 512);
}

#[test]
fn an_untaken_timing_reads_as_none_rather_than_zero() {
    let mut record = Record::new(1);
    assert_eq!(record.timing(Timing::BodyRead), None);
    record.time(Timing::BodyRead, Duration::ZERO);
    assert_eq!(record.timing(Timing::BodyRead), Some(Duration::ZERO));
}

#[test]
fn the_ring_keeps_only_its_newest_records_and_pages_skip_dropped_ones() {
    let mut ring = Ring::default();
    for id in 0..(RING as u64 + 10) {
        ring.push(Record::new(id));
    }
    let page = ring.page(0, PAGE);
    assert_eq!(page.records.len(), PAGE);
    assert_eq!(
        page.records[0].acquisition_id, 10,
        "the oldest ten are gone"
    );
    assert_eq!(page.next_cursor, 10 + PAGE as u64);
    let rest = ring.page(page.next_cursor, 100);
    assert_eq!(
        rest.records.len(),
        PAGE,
        "a page never passes its count bound"
    );
    let end = ring.page(RING as u64 + 10, PAGE);
    assert!(end.records.is_empty());
    assert_eq!(end.next_cursor, RING as u64 + 10);
}

#[test]
fn a_page_stays_within_its_byte_bound() {
    let mut ring = Ring::default();
    for id in 0..PAGE as u64 {
        let mut record = failed(u64::MAX - id, Stage::AfterProof, Reason::OtherKnownError);
        for timing in [Timing::BeforeProof, Timing::Total] {
            record.time(timing, Duration::MAX);
        }
        for field in [
            &mut record.source_generation,
            &mut record.age_at_decision_ns,
            &mut record.credit_wait_ns,
            &mut record.broker_age_ns,
            &mut record.client_bound_ns,
            &mut record.source_sequence,
            &mut record.input_revision,
            &mut record.last_good_gap_ns,
            &mut record.allocated_bytes,
            &mut record.reserved_bytes,
        ] {
            *field = u64::MAX;
        }
        ring.push(record);
    }
    let page = ring.page(0, PAGE);
    assert!(page.records.len() < PAGE, "the byte bound shortened it");
    assert!(serde_json::to_vec(&page).unwrap().len() <= PAGE_BYTES);
    assert_eq!(page.next_cursor, page.records.len() as u64);
}

#[test]
fn errors_name_their_reason_by_code_alone() {
    assert_eq!(Reason::of(Code::StaleFrame), Reason::SourceStale);
    assert_eq!(Reason::of(Code::SessionChanged), Reason::EpochChanged);
    assert_eq!(Reason::of(Code::Permission), Reason::ConsentChanged);
    assert_eq!(Reason::of(Code::Transport), Reason::TransportEnded);
    assert_eq!(Reason::of(Code::Capture), Reason::OtherKnownError);
    assert_eq!(Backend::named("cg-fallback"), Backend::CgFallback);
    assert_eq!(Backend::named("anything"), Backend::None);
}

#[test]
fn a_record_has_exactly_the_specified_keys() {
    let value = serde_json::to_value(Record::new(1)).unwrap();
    let mut keys: Vec<_> = value.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    let mut expected = vec![
        "version",
        "acquisitionID",
        "sourceGeneration",
        "requestedBackend",
        "actualBackend",
        "timingMask",
        "timingsNs",
        "ageAtDecisionNs",
        "creditWaitNs",
        "brokerAgeNs",
        "clientBoundNs",
        "sourceSequence",
        "inputRevision",
        "outcome",
        "stage",
        "reason",
        "pipeSynchronized",
        "pipeReused",
        "lastGoodGapNs",
        "releaseReason",
        "allocatedBytes",
        "reservedBytes",
        "lifecycle",
    ];
    expected.sort();
    assert_eq!(keys, expected);
}
