import AtlasUI

extension SettingsPage {
  /// The page a saved value names: the page by its own name, the tabs' old names by the page that took their place,
  /// and Helper for none or one no longer known.
  ///
  /// - Parameter saved: The value saved under ``SettingsPreferences/tabKey``, if any.
  /// - Returns: The page.
  static func saved(_ saved: String?) -> SettingsPage {
    guard let saved else { return .helper }
    if let page = SettingsPage(rawValue: saved) { return page }
    switch saved {
    case "agents": return .integrations
    default: return .helper
    }
  }
}
