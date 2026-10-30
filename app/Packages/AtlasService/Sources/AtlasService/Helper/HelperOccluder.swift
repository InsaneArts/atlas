/// One window from the helper's complete `windows --all` reply, including menus and accessory applications.
public struct HelperOccluder: Decodable, Equatable, Sendable {
  /// The window geometry; its order is assigned from the complete reply by `HelperOccluderList`.
  public var window: HelperWindow
  /// The owning process's display name, used only to recognize system pass-through overlays.
  public var owner: String?
  /// The window's opacity; fully transparent windows do not cover anything.
  public var alpha: Double

  /// Creates a window observation.
  ///
  /// - Parameters:
  ///   - window: Its identity and geometry.
  ///   - owner: Its owning process's name.
  ///   - alpha: Its opacity.
  public init(window: HelperWindow, owner: String? = nil, alpha: Double = 1) {
    self.window = window
    self.owner = owner
    self.alpha = alpha
  }

  /// The command's window fields.
  enum CodingKeys: String, CodingKey { case number, owner, alpha, bounds }

  /// The command's bounds fields.
  private struct Bounds: Decodable {
    let x: Double
    let y: Double
    let width: Double
    let height: Double
  }

  /// Decodes strictly: an incomplete observation must never silently lose an occluder.
  ///
  /// - Parameter decoder: The command's window row.
  /// - Throws: A decoding error for incomplete geometry or opacity.
  public init(from decoder: any Decoder) throws {
    let fields = try decoder.container(keyedBy: CodingKeys.self)
    let bounds = try fields.decode(Bounds.self, forKey: .bounds)
    window = HelperWindow(
      id: try fields.decode(Int.self, forKey: .number),
      x: bounds.x,
      y: bounds.y,
      width: bounds.width,
      height: bounds.height
    )
    owner = try fields.decodeIfPresent(String.self, forKey: .owner)
    alpha = try fields.decode(Double.self, forKey: .alpha)
  }
}
