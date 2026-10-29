/// What stands for an app: its own icon when one was found, else a tile drawn from its name. Never a blank square,
/// and never a bundle identifier or a path, which stay with the presenter.
public enum AppIconFace: Equatable, Sendable {
  /// The app's own icon.
  case image(IconImage)
  /// A tile drawn from its name.
  case monogram(AppMonogram)

  /// The face of an app by its name, its icon when there is one.
  ///
  /// - Parameters:
  ///   - icon: Its icon, if found.
  ///   - name: Its name, which draws the tile otherwise.
  /// - Returns: The face.
  public static func of(_ icon: IconImage?, name: String) -> AppIconFace {
    icon.map(AppIconFace.image) ?? .monogram(AppMonogram(name: name))
  }
}
