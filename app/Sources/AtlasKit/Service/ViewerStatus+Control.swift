extension ViewerStatus {
  /// Whether the service says no viewer holds control: its lease says so, or, from a service that does not say, the
  /// helper is in a mode it takes only while no viewer is in control.
  package var noViewerInControl: Bool {
    guard let controlActive else { return [.observing, .agent, .loggedOut].contains(overview.mode) }
    return !controlActive
  }
}
