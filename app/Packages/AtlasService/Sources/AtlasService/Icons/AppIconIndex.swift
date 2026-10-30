/// The app icon cache's index, kept beside its icons as `index.json`: each icon's file by bundle identifier, and the
/// names apps were seen under, so an Activity row that names only an app finds its icon. It holds a bundle identifier,
/// a name and the day an icon was last used or read, never a time of day, an action, a title or a path.
public struct AppIconIndex: Codable, Equatable, Sendable {
  /// 1: the index's shape, which a later shape changes, so an index of another starts the cache over.
  public static let currentVersion = 1

  /// The index's shape.
  public var version: Int
  /// The icons kept, by bundle identifier.
  public var icons: [String: Entry]
  /// The bundle identifier each app's name was last seen with, by the name's key (``AppIconKey/name(_:)``).
  public var names: [String: String]

  /// Creates an index.
  ///
  /// - Parameters:
  ///   - version: The index's shape.
  ///   - icons: The icons kept, by bundle identifier.
  ///   - names: The bundle identifier of each name.
  public init(version: Int = currentVersion, icons: [String: Entry] = [:], names: [String: String] = [:]) {
    self.version = version
    self.icons = icons
    self.names = names
  }

  /// The bytes of every icon kept.
  public var bytes: Int {
    icons.values
      .map(\.bytes)
      .reduce(0, +)
  }

  /// The index with only what it may hold: in its shape, each icon under a bundle identifier with a PNG's file name
  /// and a size within ``AppIconCachePolicy/largestFile``, and each name a name's key for a bundle identifier. Nil
  /// for an index of another shape, which starts the cache over.
  public var checked: AppIconIndex? {
    guard version == Self.currentVersion else { return nil }
    let icons = icons.filter { id, entry in
      AppIconKey.isBundleId(id) && entry.isWellFormed
    }
    let names = names.filter { name, id in
      AppIconKey(name: name) == .name(name) && AppIconKey.isBundleId(id)
    }
    return AppIconIndex(version: version, icons: icons, names: names)
  }
}
