extension HelperAppOrigin {
  /// How an agent's session came to open an app.
  public enum How: Equatable, Sendable {
    /// Opened by name, or from where it is installed.
    case launch
    /// Run from a copy Atlas made for the helper.
    case copy
    /// Installed into the helper by one of the session's tasks.
    case install

    /// The way the service's word names.
    ///
    /// - Parameter word: The service's `how`, such as `copy`.
    public init(word: String?) {
      switch word {
      case "copy": self = .copy
      case "install": self = .install
      default: self = .launch
      }
    }
  }
}
