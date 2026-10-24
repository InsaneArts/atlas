/// AsyncSupportTesting — level 0, the one product of `app/Packages/AsyncSupport`, imported by test targets only.
/// Nothing here ships.
///
/// Owns: the waits every Atlas test target shares, none of which sleeps: `ManualClock`, the clock a test moves by
/// hand where the app passes `ContinuousClock()`, and `untilSleeping`, which waits for its sleepers; `settle`, which
/// waits for a condition over observable state and reads it again only when that state changes; `afterQueuedWork`,
/// the sentinel that proves an absence; and `Task.value(_:within:)`, which waits for a task's value. Each is bounded
/// by `Ceiling`, whose timer is the tests' one wall-clock wait. It holds no fakes.
/// Never imports: anything but Testing, Observation and os.
