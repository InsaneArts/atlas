extension DesktopList {
  /// One desktop the service serves.
  public struct Entry: Decodable, Equatable, Sendable {
    /// Its ID, which every request names it by.
    public var id: String
    /// The name its records carry.
    public var key: String
    /// The settings' name for it.
    public var name: String
    /// What kind of desktop it is.
    public var kind: Kind
    /// Whether it is the settings' default.
    public var isDefault = false
    /// Whether the app views it.
    public var isViewed = false
    /// What it is doing, as its newest status read said; nil while none was read.
    public var standing: OverviewState?
    /// What the person can do with a VM now, such as `start` or `stop`, as the service decides it.
    public var can: [String] = []

    /// Creates an entry.
    ///
    /// - Parameters:
    ///   - id: Its ID.
    ///   - key: The name its records carry.
    ///   - name: The settings' name for it.
    ///   - kind: What kind of desktop it is.
    ///   - isDefault: Whether it is the default.
    ///   - isViewed: Whether the app views it.
    public init(id: String, key: String, name: String, kind: Kind, isDefault: Bool = false, isViewed: Bool = false) {
      self.id = id
      self.key = key
      self.name = name
      self.kind = kind
      self.isDefault = isDefault
      self.isViewed = isViewed
    }

    /// The fields of one desktop.
    private enum CodingKeys: String, CodingKey {
      case id, key, name, kind, standing, facts
      case isDefault = "default"
      case isViewed = "viewed"
    }

    /// The fields of a VM's facts this entry reads.
    private enum FactsKeys: String, CodingKey {
      case can
    }

    /// Decodes a desktop: one without an ID is not one.
    ///
    /// - Parameter decoder: The decoder reading one desktop.
    /// - Throws: `DecodingError` when it is not an object or names no ID.
    public init(from decoder: any Decoder) throws {
      let container = try decoder.container(keyedBy: CodingKeys.self)
      id = try container.decode(String.self, forKey: .id)
      key = container.lenientText(.key) ?? ""
      name = container.lenientText(.name) ?? ""
      kind = Kind(word: container.lenientText(.kind) ?? "")
      isDefault = container.lenientFlag(.isDefault)
      isViewed = container.lenientFlag(.isViewed)
      standing = container.lenientText(.standing)
        .map(OverviewState.init(name:))
      guard let facts = try? container.nestedContainer(keyedBy: FactsKeys.self, forKey: .facts) else { return }
      can = facts.lenientList(String.self, forKey: .can)
    }
  }
}
