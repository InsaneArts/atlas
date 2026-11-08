import Foundation

/// A picture of the helper's screen taken right before or right after an agent's action, known by Atlas's own file
/// name, `<nanoseconds>[-<session>[-<task>]]-before.jpg` or the same ending `-after.jpg`. `screenshot.png` serves it by
/// that name, though the screenshots never list it.
public enum ActionPicture {
  /// How a picture taken before an action ends.
  static let before = "-before.jpg"
  /// How a picture taken after an action ends.
  static let after = "-after.jpg"

  /// When the picture named `name` was taken, read from its name.
  ///
  /// - Parameter name: A file name.
  /// - Returns: The moment, or nil for any name but such a picture's.
  public static func takenAt(_ name: String) -> Date? {
    guard let suffix = [before, after].first(where: name.hasSuffix) else { return nil }
    let parts = name.dropLast(suffix.count)
      .split(separator: "-", omittingEmptySubsequences: false)
    guard let digits = parts.first, parts.count <= 3, !digits.isEmpty,
      digits.allSatisfy({ $0.isASCII && $0.isNumber }),
      parts.dropFirst().allSatisfy(isKey),
      let nanoseconds = Double(digits)
    else { return nil }
    return Date(timeIntervalSince1970: nanoseconds / 1_000_000_000)
  }

  /// Whether `name` is such a picture's.
  ///
  /// - Parameter name: A file name.
  /// - Returns: Whether it is.
  public static func isName(_ name: String) -> Bool {
    takenAt(name) != nil
  }

  /// Whether `name` is a picture Atlas took after an action, rather than the next action's picture before.
  ///
  /// - Parameter name: A file name.
  /// - Returns: Whether it is.
  public static func isAfter(_ name: String) -> Bool {
    name.hasSuffix(after) && isName(name)
  }

  /// Whether `part` of a name is a session's or task's key: twelve lowercase hex digits.
  ///
  /// - Parameter part: The part.
  /// - Returns: Whether it is one.
  static func isKey(_ part: Substring) -> Bool {
    part.count == 12 && part.allSatisfy { $0.isASCII && ($0.isNumber || ("a"..."f").contains($0)) }
  }
}
