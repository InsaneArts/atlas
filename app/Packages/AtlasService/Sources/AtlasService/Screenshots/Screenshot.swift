import Foundation

/// A screenshot an agent took in the helper, as the viewer service's `screenshots.json` lists it, newest first.
/// Every field may be missing, so each has a safe default.
public struct Screenshot: Decodable, Equatable, Sendable, Identifiable {
  /// The screenshot's file name, which `screenshot.png` takes; empty when the service gives none.
  public var name: String
  /// When it was taken, when the service says.
  public var takenAt: Date?
  /// Its width in pixels, when its header reads.
  public var width: Int?
  /// Its height in pixels, when its header reads.
  public var height: Int?
  /// The key of the host session that took it; nil when no host named one, never empty.
  public var session: String?
  /// The key of the host session that took it: Terminal's empty key when no host named one.
  public var sessionKey: String { session ?? "" }

  /// The screenshot's identity in a list: its file name.
  public var id: String { name }

  /// Creates a screenshot.
  ///
  /// - Parameters:
  ///   - name: Its file name.
  ///   - takenAt: When it was taken.
  ///   - width: Its width in pixels.
  ///   - height: Its height in pixels.
  ///   - session: The key of the host session that took it.
  public init(name: String, takenAt: Date? = nil, width: Int? = nil, height: Int? = nil, session: String? = nil) {
    self.name = name
    self.takenAt = takenAt
    self.width = width
    self.height = height
    self.session = session
  }

  /// The fields of one `screenshots` row.
  enum CodingKeys: String, CodingKey {
    /// Each field by the name the service writes.
    case name, takenAt, width, height, session
  }

  /// Decodes a screenshot, keeping a safe default for anything missing or of the wrong type. `takenAt` is read as
  /// milliseconds since 1970.
  ///
  /// - Parameter decoder: The decoder reading one `screenshots` row.
  /// - Throws: `DecodingError` only when the row is not an object.
  public init(from decoder: any Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    name = (try? container.decodeIfPresent(String.self, forKey: .name)) ?? ""
    takenAt = container.lenientDate(milliseconds: .takenAt)
    width = try? container.decodeIfPresent(Int.self, forKey: .width)
    height = try? container.decodeIfPresent(Int.self, forKey: .height)
    // An empty key is Terminal's, as a missing one is, so both read as none.
    session = (try? container.decodeIfPresent(String.self, forKey: .session))
      .flatMap { $0?.isEmpty == false ? $0 : nil }
  }

  /// When a capture was taken, read from its file name as Atlas writes it, nanoseconds since 1970 then a dash, such as
  /// `1793163300000000000-capture.png`: for a screenshot a list names before the screenshots are read.
  ///
  /// - Parameter name: The capture's file name.
  /// - Returns: The time, or nil for a name that does not start so.
  public static func takenAt(named name: String) -> Date? {
    let digits = name.prefix { $0.isASCII && $0.isNumber }
    guard name.dropFirst(digits.count).first == "-", let nanoseconds = Double(digits), nanoseconds > 0 else {
      return nil
    }
    return Date(timeIntervalSince1970: nanoseconds / 1_000_000_000)
  }
}
