import AtlasService
import AtlasUI

extension ViewerPresenter {
  /// What stands for an app: the icon found for it, from the open app, the icons kept between launches or where it
  /// is installed, by its bundle identifier, the one its name was seen with, or its name; else the tile drawn from
  /// its name, which also shows while its icon is still being found. Never a blank square, and never a path.
  ///
  /// - Parameters:
  ///   - bundleId: The app's bundle identifier, if known.
  ///   - name: The app's name, as shown.
  ///   - icons: The icons found so far.
  /// - Returns: The icon or the tile.
  static func appIcon(bundleId: String?, name: String, _ icons: ViewerIconsFacts) -> AppIconFace {
    .of(icons.image(bundleId: bundleId, name: name), name: name)
  }
}
