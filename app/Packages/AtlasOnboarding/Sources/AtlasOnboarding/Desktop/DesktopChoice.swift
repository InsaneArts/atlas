/// One row of the setup's choice of where agents work: a kind of desktop, and why it cannot be chosen now, if it
/// cannot.
public struct DesktopChoice: Equatable, Sendable {
  /// The kind of desktop.
  public let kind: Kind
  /// Why it cannot be chosen now; nil when it can.
  public let lack: Lack?

  /// Whether it can be chosen now.
  public var canBeChosen: Bool { lack == nil }

  /// The three kinds in their order, each as `facts` say it can be chosen today: Second Login on a first setup, never
  /// while another desktop is being added, a VM when this Mac can run one, Cloud not yet.
  ///
  /// - Parameters:
  ///   - facts: What `atlas onboarding status` reported last.
  ///   - adding: Whether the window adds a desktop beside those set up, as Settings' Add a Desktop… asks; not by
  ///     default.
  /// - Returns: The rows, in order.
  public static func all(_ facts: OnboardingFacts, adding: Bool = false) -> [DesktopChoice] {
    Kind.allCases
      .map { kind in
        switch kind {
        // Settings adds a VM beside a Second Login, listed even when its account is gone; settings that list VMs
        // alone name no helper, and add none here.
        case .secondLogin:
          DesktopChoice(
            kind: kind,
            lack: adding ? (facts.helper.exists || facts.helper.uid != 0 ? .added : .notYet) : nil
          )
        case .vm: DesktopChoice(kind: kind, lack: facts.vmLack)
        case .cloud: DesktopChoice(kind: kind, lack: .notYet)
        }
      }
  }

  /// The kind picked as the choice opens or once a pick can no longer be chosen: `picked` while it can be, else the
  /// first that can, Second Login on a first setup; nil when none can, as when Settings adds a desktop on a Mac that
  /// cannot run a VM.
  ///
  /// - Parameters:
  ///   - picked: The kind the person picked last.
  ///   - facts: What `atlas onboarding status` reported last.
  ///   - adding: Whether the window adds a desktop beside those set up; not by default.
  /// - Returns: The kind the choice shows picked, or nil.
  public static func shown(_ picked: Kind, facts: OnboardingFacts, adding: Bool = false) -> Kind? {
    let rows = all(facts, adding: adding)
    if rows.contains(where: { $0.kind == picked && $0.canBeChosen }) { return picked }
    return rows.first(where: \.canBeChosen)?.kind
  }
}
