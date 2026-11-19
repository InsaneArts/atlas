/// Every desktop the service serves, as `desktops.json` lists them, and the one the app views.
public struct DesktopList: Decodable, Equatable, Sendable {
  /// The viewed desktop's ID; nil when the service named none.
  public var viewed: String?
  /// Every desktop, in the settings' order.
  public var desktops: [Entry] = []

  /// Creates a list.
  ///
  /// - Parameters:
  ///   - viewed: The viewed desktop's ID.
  ///   - desktops: Every desktop, in the settings' order.
  public init(viewed: String? = nil, desktops: [Entry] = []) {
    self.viewed = viewed
    self.desktops = desktops
  }

  /// The fields of `desktops.json`.
  private enum CodingKeys: String, CodingKey {
    case viewed, desktops
  }

  /// Decodes a list, keeping the default of anything missing or of the wrong type; a desktop that does not read is
  /// left out.
  ///
  /// - Parameter decoder: The decoder reading the reply.
  /// - Throws: `DecodingError` only when it is not an object.
  public init(from decoder: any Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    viewed = container.lenientText(.viewed)
    desktops = container.lenientList(Entry.self, forKey: .desktops)
  }
}
