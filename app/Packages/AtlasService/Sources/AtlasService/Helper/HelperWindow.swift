/// A window of an app open in the helper, as `apps.json` lists it. Every field may be missing, so each has a safe
/// default.
public struct HelperWindow: Decodable, Equatable, Sendable, Identifiable {
  /// The window's number in the helper's session; 0 when the service gives none.
  public var id: Int
  /// The window's title, when it has one.
  public var title: String?
  /// The window's left edge on the helper's display, in points.
  public var x: Double
  /// The window's top edge on the helper's display, in points.
  public var y: Double
  /// The window's width, in points.
  public var width: Double
  /// The window's height, in points.
  public var height: Double
  /// The window's place among every app's windows, front to back, from 0; nil when the service gives none.
  public var order: Int?

  /// Creates a window.
  ///
  /// - Parameters:
  ///   - id: The window's number.
  ///   - title: The window's title.
  ///   - x: Its left edge, in points.
  ///   - y: Its top edge, in points.
  ///   - width: Its width, in points.
  ///   - height: Its height, in points.
  ///   - order: Its place among every app's windows, front to back.
  public init(
    id: Int,
    title: String? = nil,
    x: Double = 0,
    y: Double = 0,
    width: Double = 0,
    height: Double = 0,
    order: Int? = nil
  ) {
    self.id = id
    self.title = title
    self.x = x
    self.y = y
    self.width = width
    self.height = height
    self.order = order
  }

  /// The fields of one `windows` row.
  enum CodingKeys: String, CodingKey {
    /// Each field by the name the service writes.
    case id, title, x, y, width, height, order
  }

  /// Decodes a window, keeping a safe default for anything missing or of the wrong type.
  ///
  /// - Parameter decoder: The decoder reading one `windows` row.
  /// - Throws: `DecodingError` only when the row is not an object.
  public init(from decoder: any Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    let number = { (key: CodingKeys) in (try? container.decodeIfPresent(Double.self, forKey: key)) ?? 0 }
    id = (try? container.decodeIfPresent(Int.self, forKey: .id)) ?? 0
    title = try? container.decodeIfPresent(String.self, forKey: .title)
    x = number(.x)
    y = number(.y)
    width = number(.width)
    height = number(.height)
    order = (try? container.decodeIfPresent(Int.self, forKey: .order))
      .flatMap { $0 >= 0 ? $0 : nil }
  }
}
