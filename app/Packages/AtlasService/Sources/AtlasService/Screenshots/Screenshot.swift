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

  /// The screenshot's identity in a list: its file name.
  public var id: String { name }

  /// Creates a screenshot.
  ///
  /// - Parameters:
  ///   - name: Its file name.
  ///   - takenAt: When it was taken.
  ///   - width: Its width in pixels.
  ///   - height: Its height in pixels.
  public init(name: String, takenAt: Date? = nil, width: Int? = nil, height: Int? = nil) {
    self.name = name
    self.takenAt = takenAt
    self.width = width
    self.height = height
  }

  /// The fields of one `screenshots` row.
  enum CodingKeys: String, CodingKey {
    case name, takenAt, width, height
  }

  /// Decodes a screenshot, keeping a safe default for anything missing or of the wrong type. `takenAt` is read as
  /// milliseconds since 1970.
  ///
  /// - Parameter decoder: The decoder reading one `screenshots` row.
  /// - Throws: `DecodingError` only when the row is not an object.
  public init(from decoder: any Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    name = (try? container.decodeIfPresent(String.self, forKey: .name)) ?? ""
    let milliseconds = try? container.decodeIfPresent(Double.self, forKey: .takenAt)
    takenAt = milliseconds.map { Date(timeIntervalSince1970: $0 / 1000) }
    width = try? container.decodeIfPresent(Int.self, forKey: .width)
    height = try? container.decodeIfPresent(Int.self, forKey: .height)
  }
}
