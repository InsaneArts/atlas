extension HelperMode {
  /// Logging out needs a helper that answers with nothing left to settle.
  package var allowsLogOut: Bool { [.observing, .controlling, .blocked].contains(self) }

  /// Logging out anyway also settles a log-out that an app held up.
  package var allowsForcedLogOut: Bool { allowsLogOut || self == .recovering }
}
