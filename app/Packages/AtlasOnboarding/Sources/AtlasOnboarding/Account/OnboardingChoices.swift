/// What the person has chosen in the window, which the facts cannot say.
public struct OnboardingChoices: Equatable, Sendable {
  /// Past the welcome screen.
  public var started = false
  /// Where agents work, chosen with Continue on the choice; nil until then.
  public var desktop: DesktopChoice.Kind?
  /// The VM's check passed and the person went on past it.
  public var checked = false
  /// The window opened from Settings' Add a Desktop…: it chooses a desktop to add beside those set up, and follows
  /// that add.
  public var adding = false
  /// The VM this window's add made, by its name, once that add ended: its check comes next.
  public var added: String?
  /// On the account form rather than the list.
  public var creating = false
  /// The agents screen is behind them.
  public var agentsDone = false
  /// Atlas just closed the logins deleted accounts left open: the close screen says so until Continue.
  public var closedLeftovers = false

  /// Creates choices with nothing chosen.
  public init() {}
}
