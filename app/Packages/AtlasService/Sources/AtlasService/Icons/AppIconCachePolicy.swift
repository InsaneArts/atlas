import Foundation

/// The app icon cache's bounds: what it refuses, how much it keeps, which icons go first past its limits, and when an
/// icon is looked up again. Pure, so a test reads each rule as it is.
public enum AppIconCachePolicy {
  /// 128: an icon's side in pixels, twice the 44-point card's, so it is sharp on a Retina display.
  public static let side = 128
  /// 256 KB: the largest icon file kept or read back.
  public static let largestFile = 256 * 1024
  /// 512: the largest side an icon may decode to, so a file that unpacks into a huge picture is refused.
  public static let largestSide = 512
  /// 512: the most icons kept.
  public static let mostIcons = 512
  /// 16 MB: the most bytes of icons kept.
  public static let mostBytes = 16 * 1024 * 1024
  /// 400: the icons left once the cache has gone past either limit, so it is not trimmed again with the next icon.
  public static let keptIcons = 400
  /// 12 MB: the bytes left once the cache has gone past either limit.
  public static let keptBytes = 12 * 1024 * 1024
  /// 30: the days after which an icon found where an app is installed is looked up again, in case the app changed.
  public static let lookAgainDays = 30
  /// 1 MB: the largest index read back; one of ``mostIcons`` entries and as many names is a small part of it.
  public static let largestIndex = 1024 * 1024
  /// 86 400: the seconds in a day, which turn a time into the day the index keeps.
  static let secondsADay: TimeInterval = 86_400

  /// The day a time falls on, counted from 1970: all the cache keeps of when an icon was used.
  ///
  /// - Parameter date: The time.
  /// - Returns: Its day.
  public static func day(of date: Date) -> Int {
    Int((date.timeIntervalSince1970 / secondsADay).rounded(.down))
  }

  /// Whether an icon's file is small enough to keep or read.
  ///
  /// - Parameter bytes: The file's size.
  /// - Returns: Whether it is within ``largestFile``.
  public static func accepts(bytes: Int) -> Bool {
    bytes > 0 && bytes <= largestFile
  }

  /// Whether an icon's picture is small enough to keep or show.
  ///
  /// - Parameters:
  ///   - width: Its width in pixels.
  ///   - height: Its height in pixels.
  /// - Returns: Whether neither side passes ``largestSide``.
  public static func accepts(width: Int, height: Int) -> Bool {
    width > 0 && height > 0 && width <= largestSide && height <= largestSide
  }

  /// Whether an icon kept since `read` is looked up again where its app is installed.
  ///
  /// - Parameters:
  ///   - entry: The icon's entry.
  ///   - today: Today's day.
  /// - Returns: Whether ``lookAgainDays`` have passed since it was last read.
  public static func looksAgain(_ entry: AppIconIndex.Entry, today: Int) -> Bool {
    guard today >= entry.read else { return false }
    let age = today.subtractingReportingOverflow(entry.read)
    return age.overflow || age.partialValue >= lookAgainDays
  }

  /// Cache dates run from 1970 through Foundation's distant future. Refuse corrupt persisted integers before use.
  static func accepts(day: Int) -> Bool {
    (0...Self.day(of: .distantFuture))
      .contains(day)
  }

  /// The icons to drop once the index has gone past ``mostIcons`` or ``mostBytes``: the least recently used first,
  /// until ``keptIcons`` and ``keptBytes`` are left; none while it is within both.
  ///
  /// - Parameter index: The index.
  /// - Returns: The bundle identifiers to drop.
  public static func evicted(_ index: AppIconIndex) -> [String] {
    var count = index.icons.count
    var bytes = index.bytes
    guard count > mostIcons || bytes > mostBytes else { return [] }
    // Oldest use first, and of one day the larger first; the identifier settles ties, so the choice never varies.
    let order = index.icons.sorted { lhs, rhs in
      if lhs.value.used != rhs.value.used { return lhs.value.used < rhs.value.used }
      if lhs.value.bytes != rhs.value.bytes { return lhs.value.bytes > rhs.value.bytes }
      return lhs.key < rhs.key
    }
    var dropped: [String] = []
    for (id, entry) in order where count > keptIcons || bytes > keptBytes {
      dropped.append(id)
      count -= 1
      bytes -= entry.bytes
    }
    return dropped
  }

  /// The names to forget once there are more than ``mostIcons``: first those whose app has no icon kept, then the
  /// rest by name, until ``keptIcons`` are left.
  ///
  /// - Parameter index: The index.
  /// - Returns: The names' keys to forget.
  public static func forgottenNames(_ index: AppIconIndex) -> [String] {
    guard index.names.count > mostIcons else { return [] }
    let order = index.names.sorted { lhs, rhs in
      let lhsKept = index.icons[lhs.value] != nil
      let rhsKept = index.icons[rhs.value] != nil
      if lhsKept != rhsKept { return !lhsKept }
      return lhs.key < rhs.key
    }
    return order.prefix(index.names.count - keptIcons)
      .map(\.key)
  }
}
