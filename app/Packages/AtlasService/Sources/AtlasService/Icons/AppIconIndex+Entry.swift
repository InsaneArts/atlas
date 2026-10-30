extension AppIconIndex {
  /// One icon kept: its file, the app's name, the days it was last used and last read from the app, its size and its
  /// digest. Days count from 1970, so the index never says when in a day an app was used.
  public struct Entry: Codable, Equatable, Sendable {
    /// What ends an icon's file name.
    public static let fileSuffix = ".png"
    /// 16: the hexadecimal digits of an icon's file name, cut from its bundle identifier's digest.
    public static let fileDigits = 16

    /// The icon's file in the cache's folder, such as `0123456789abcdef.png`.
    public var file: String
    /// The app's name when it was read, if known.
    public var name: String?
    /// The day the icon was last shown.
    public var used: Int
    /// The day it was last read from the app itself, which says when to look it up again.
    public var read: Int
    /// The PNG's size, in bytes.
    public var bytes: Int
    /// The PNG's digest, in hexadecimal, which says whether a newer read changed it.
    public var sha: String

    /// Creates an entry.
    ///
    /// - Parameters:
    ///   - file: The icon's file in the cache's folder.
    ///   - name: The app's name.
    ///   - used: The day the icon was last shown.
    ///   - read: The day it was last read from the app.
    ///   - bytes: The PNG's size.
    ///   - sha: The PNG's digest.
    public init(file: String, name: String?, used: Int, read: Int, bytes: Int, sha: String) {
      self.file = file
      self.name = name
      self.used = used
      self.read = read
      self.bytes = bytes
      self.sha = sha
    }

    /// Whether the entry names a PNG of ``fileDigits`` hexadecimal digits in the cache's own folder, and a size the
    /// cache would keep, with valid persisted days.
    var isWellFormed: Bool {
      guard file.hasSuffix(Self.fileSuffix) else { return false }
      let stem = file.dropLast(Self.fileSuffix.count)
      return stem.count == Self.fileDigits && stem.allSatisfy(\.isHexDigit) && bytes > 0
        && bytes <= AppIconCachePolicy.largestFile
        && AppIconCachePolicy.accepts(day: used) && AppIconCachePolicy.accepts(day: read)
    }
  }
}
