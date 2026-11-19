/// The VM a first setup makes, as `atlas onboarding status` reports it from `atlas desktop list`: how far its add got
/// and whether it still goes on, and whether the VM is ready for agents.
public struct VMFacts: Equatable, Sendable {
  /// Its name in the settings.
  public var name = ""
  /// Whether it is still being added; false once added.
  var adding = false
  /// The steps of its add done.
  var steps: Set<Step> = []
  /// How far making it got, as Atlas VM says; nil when it does not say.
  public var progress: Progress?
  /// Where the setup of its add stands: its window open and waiting, resuming, or done; nil before its window opened.
  var setup: Setup?
  /// Whether the guest said Screen & System Audio Recording is allowed in the setup window.
  var recordingAllowed = false
  /// Whether the guest said Accessibility is allowed in the setup window.
  var accessibilityAllowed = false
  /// Why its add last stopped short, in its own words; nil when it did not.
  public var failure: String?
  /// Whether the process making its add still runs; nil once it is added, or when that cannot be told.
  var adderRunning: Bool?
  /// Whether it is ready for agents.
  public var ready = false

  /// Creates the facts of a VM with nothing known.
  public init() {}
}
