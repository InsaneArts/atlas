/// One row of the setup's choice of where agents work: a kind of desktop, and why it cannot be chosen now, if it
/// cannot.
public struct DesktopChoice: Equatable, Sendable {
  /// The kind of desktop.
  public let kind: Kind
  /// Why it cannot be chosen now; nil when it can.
  public let lack: Lack?

  /// Whether it can be chosen now.
  public var canBeChosen: Bool { lack == nil }

  /// The three kinds in their order, each as `facts` say it can be chosen today: Second Login always, a VM when this
  /// Mac can run one, Cloud not yet.
  ///
  /// - Parameter facts: What `atlas onboarding status` reported last.
  /// - Returns: The rows, in order.
  public static func all(_ facts: OnboardingFacts) -> [DesktopChoice] {
    Kind.allCases
      .map { kind in
        switch kind {
        case .secondLogin: DesktopChoice(kind: kind, lack: nil)
        case .vm: DesktopChoice(kind: kind, lack: facts.vmLack)
        case .cloud: DesktopChoice(kind: kind, lack: .notYet)
        }
      }
  }

  /// The kind picked as the choice opens or once a pick can no longer be chosen: `picked` while it can be, else
  /// Second Login, which always can.
  ///
  /// - Parameters:
  ///   - picked: The kind the person picked last.
  ///   - facts: What `atlas onboarding status` reported last.
  /// - Returns: The kind the choice shows picked.
  public static func shown(_ picked: Kind, facts: OnboardingFacts) -> Kind {
    let row = all(facts)
      .first { $0.kind == picked }
    return row?.canBeChosen == true ? picked : .secondLogin
  }
}
