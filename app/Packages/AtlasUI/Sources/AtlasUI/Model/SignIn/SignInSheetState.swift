/// Sign In to a Site…'s sheet: the site the person typed, why the last Continue was refused, and whether one runs now.
public struct SignInSheetState: Equatable, Sendable {
  /// The site as typed, such as `github.com`.
  public var site: String
  /// Why Atlas couldn't start, such as "An agent's command is still running. Try again when it ends."; nil for
  /// nothing.
  public var refusal: String?
  /// Whether Continue runs now, so the buttons wait.
  public var isWorking: Bool

  /// Creates the sheet's state.
  ///
  /// - Parameters:
  ///   - site: The site as typed; empty by default.
  ///   - refusal: Why the last Continue was refused; none by default.
  ///   - isWorking: Whether Continue runs now; not by default.
  public init(site: String = "", refusal: String? = nil, isWorking: Bool = false) {
    self.site = site
    self.refusal = refusal
    self.isWorking = isWorking
  }

  /// Whether Continue can be pressed: a site is typed and nothing runs.
  public var canContinue: Bool {
    !site.trimmingCharacters(in: .whitespaces).isEmpty && !isWorking
  }
}
