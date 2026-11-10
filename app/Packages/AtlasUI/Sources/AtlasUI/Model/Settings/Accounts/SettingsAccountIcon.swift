/// What an account's tile shows in place of its first letter: its app's own icon, or its site's icon.
public enum SettingsAccountIcon: Equatable, Sendable {
  /// A Mac app's own icon, drawn as the Dock draws it.
  case app(IconImage)
  /// A site's icon, drawn on a white tile, as a web app's icon stands in the Dock.
  case site(IconImage)
}
