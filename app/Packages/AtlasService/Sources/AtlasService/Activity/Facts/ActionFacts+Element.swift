extension ActionFacts {
  /// The element an action went to, such as the button under the pointer or the field with focus: where it stands,
  /// and what it is and is called when known.
  public struct Element: Decodable, Equatable, Sendable {
    /// Where it stands on the display.
    public var frame: Frame
    /// What it is, such as `AXButton`; nil when unknown.
    public var role: String?
    /// What it is called, such as "Send"; nil when unknown.
    public var name: String?

    /// Creates an element.
    ///
    /// - Parameters:
    ///   - frame: Where it stands on the display.
    ///   - role: What it is; none by default.
    ///   - name: What it is called; none by default.
    public init(frame: Frame, role: String? = nil, name: String? = nil) {
      self.frame = frame
      self.role = role
      self.name = name
    }

    /// The fields of an element.
    enum CodingKeys: String, CodingKey {
      /// Each by the name the service writes.
      case frame, role, name
    }

    /// Decodes an element, an odd role or name reading as none.
    ///
    /// - Parameter decoder: The decoder.
    /// - Throws: `DecodingError` when its frame is missing or odd, as an element with no place marks nothing.
    public init(from decoder: any Decoder) throws {
      let container = try decoder.container(keyedBy: CodingKeys.self)
      frame = try container.decode(Frame.self, forKey: .frame)
      role = container.lenientText(.role)
      name = container.lenientText(.name)
    }
  }
}
