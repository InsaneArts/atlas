extension DesktopList {
  /// What the service's newest status read of a VM said, beside what the person can do with it: each nil while it
  /// was not said, never read as running, as up to date or as nothing used.
  public struct Facts: Decodable, Equatable, Sendable {
    /// Its power as its keeper last saw Atlas VM say it: `running`, `starting`, `stopping`, `stopped` or another word.
    public var power: String?
    /// Its screen's size, while its session proves one.
    public var display: ViewerStatus.Size?
    /// How many bytes of its disk are used.
    public var diskUsed: Int64?
    /// How large its disk may grow, in bytes.
    public var diskSize: Int64?
    /// Its macOS version, as the guest says it, such as `27.2.1`.
    public var macOS: String?
    /// The newest macOS this Mac can install in a VM, as Atlas VM says it.
    public var latestMacOS: String?
    /// Whether that one is strictly newer than its own; nil while either is not known.
    public var newerMacOS: Bool?
    /// Where its maintenance stands: `none`, `on`, `ending` or `unknown`.
    public var maintenance: String?
    /// How its Atlas stands to this Mac's: `current`, `available`, `newer`, `updating` or `unknown`.
    public var atlas: String?
    /// What brings its Atlas to this Mac's, in the station's words, when it is not this Mac's.
    public var atlasNext: String?

    /// Creates facts.
    ///
    /// - Parameters:
    ///   - power: Its power; not said by default.
    ///   - display: Its screen's size; not said by default.
    ///   - diskUsed: How many bytes of its disk are used; not said by default.
    ///   - diskSize: How large its disk may grow; not said by default.
    ///   - macOS: Its macOS version; not said by default.
    ///   - latestMacOS: The newest macOS this Mac can install; not said by default.
    ///   - newerMacOS: Whether that one is newer; not known by default.
    ///   - maintenance: Where its maintenance stands; not said by default.
    ///   - atlas: How its Atlas stands; not said by default.
    ///   - atlasNext: What brings its Atlas to this Mac's; none by default.
    public init(
      power: String? = nil,
      display: ViewerStatus.Size? = nil,
      diskUsed: Int64? = nil,
      diskSize: Int64? = nil,
      macOS: String? = nil,
      latestMacOS: String? = nil,
      newerMacOS: Bool? = nil,
      maintenance: String? = nil,
      atlas: String? = nil,
      atlasNext: String? = nil
    ) {
      self.power = power
      self.display = display
      self.diskUsed = diskUsed
      self.diskSize = diskSize
      self.macOS = macOS
      self.latestMacOS = latestMacOS
      self.newerMacOS = newerMacOS
      self.maintenance = maintenance
      self.atlas = atlas
      self.atlasNext = atlasNext
    }

    /// The facts' fields this app reads.
    private enum CodingKeys: String, CodingKey {
      case power, display, disk, macos, maintenance, atlas
    }

    /// The fields of its disk.
    private enum DiskKeys: String, CodingKey {
      case used, size
    }

    /// The fields of its macOS.
    private enum MacOSKeys: String, CodingKey {
      case version, latest, newer
    }

    /// The fields of a state said with its next step: its maintenance's, its Atlas's, the newest macOS's version.
    private enum SaidKeys: String, CodingKey {
      case state, next, version
    }

    /// Decodes the facts, keeping the default of anything missing or of the wrong type.
    ///
    /// - Parameter decoder: The decoder reading the facts.
    /// - Throws: `DecodingError` only when they are not an object.
    public init(from decoder: any Decoder) throws {
      let container = try decoder.container(keyedBy: CodingKeys.self)
      power = container.lenientText(.power)
      display = (try? container.decodeIfPresent(ViewerStatus.Size.self, forKey: .display))
        .flatMap { $0.isValid ? $0 : nil }
      if let disk = try? container.nestedContainer(keyedBy: DiskKeys.self, forKey: .disk) {
        diskUsed = try? disk.decodeIfPresent(Int64.self, forKey: .used)
        diskSize = try? disk.decodeIfPresent(Int64.self, forKey: .size)
      }
      if let macos = try? container.nestedContainer(keyedBy: MacOSKeys.self, forKey: .macos) {
        macOS = macos.lenientText(.version)
        newerMacOS = macos.lenientBool(.newer)
        latestMacOS = (try? macos.nestedContainer(keyedBy: SaidKeys.self, forKey: .latest))?
          .lenientText(.version)
      }
      maintenance = (try? container.nestedContainer(keyedBy: SaidKeys.self, forKey: .maintenance))?
        .lenientText(.state)
      if let said = try? container.nestedContainer(keyedBy: SaidKeys.self, forKey: .atlas) {
        atlas = said.lenientText(.state)
        atlasNext = said.lenientText(.next)
      }
    }
  }
}
