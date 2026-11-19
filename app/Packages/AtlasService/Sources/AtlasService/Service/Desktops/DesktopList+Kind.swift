extension DesktopList {
  /// What kind of desktop one is.
  public enum Kind: Equatable, Sendable {
    /// Second Login: a hidden second login on this Mac.
    case user
    /// A virtual Mac on this Mac.
    case vm
    /// A kind this app does not know, by its word.
    case unknown(String)

    /// The kind the service names `word`.
    ///
    /// - Parameter word: The kind's word, as `desktops.json` gives it.
    init(word: String) {
      self =
        switch word {
        case "user": .user
        case "vm": .vm
        default: .unknown(word)
        }
    }
  }
}
