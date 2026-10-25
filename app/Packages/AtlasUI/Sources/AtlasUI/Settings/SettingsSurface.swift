/// One presentation in the page area, including transient search results.
enum SettingsSurface: Equatable {
  /// A root or detail page.
  case page(SettingsDestination)
  /// The local search results.
  case search

  /// The page allowed to finish fading, or nil when the outgoing presentation is search.
  var destination: SettingsDestination? {
    if case .page(let destination) = self { return destination }
    return nil
  }
}
