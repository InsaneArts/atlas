/// A sign-in Atlas was holding when it stopped: the helper's screen stays hidden from agents and their changes wait
/// until the person finishes or cancels it.
public struct PanelInterruptedSignIn: Equatable, Sendable {
  /// The site, when Atlas can still tell; nil when it can't.
  public var site: String?
  /// Whether Done is offered: only for a sign-in Atlas can still tell.
  public var canFinish: Bool
  /// What the answer names it by; nil while Atlas has not taken it over, which offers no button.
  public var id: String?

  /// Creates an interrupted sign-in note.
  ///
  /// - Parameters:
  ///   - site: The site, when known.
  ///   - canFinish: Whether Done is offered.
  ///   - id: What the answer names it by.
  public init(site: String?, canFinish: Bool, id: String?) {
    self.site = site
    self.canFinish = canFinish
    self.id = id
  }
}
