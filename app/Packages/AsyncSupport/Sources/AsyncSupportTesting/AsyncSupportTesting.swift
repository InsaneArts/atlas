/// AsyncSupportTesting — level 0, the one product of `app/Packages/AsyncSupport`, imported by test targets only.
/// Nothing here ships.
///
/// Owns: the waits every Atlas test target shares: `Ceiling`, the bound on a test's wait for background work;
/// `settle`, which waits for a condition within it; and `Task.value(_:within:)`, which waits for a task's value within
/// it. It holds no fakes.
/// Never imports: anything but Testing.
