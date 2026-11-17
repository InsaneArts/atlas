import Foundation

/// A request a command made of the person, as `atlas lifecycle request` reads it: what it asks, for which VM, from
/// which session, and by when it must be answered. The command line made it and judges its answer; the app only
/// shows it.
public struct LifecycleRequest: Decodable, Equatable, Sendable {
  /// The request's ID, 48 lowercase hexadecimal digits.
  public let id: String
  /// What it asks.
  public let kind: LifecycleKind
  /// The VM's name in the settings, such as `work`.
  public let name: String
  /// The VM's identifier.
  public let vm: String
  /// The settings file it changes, as the person writes it, such as `~/.config/atlas.toml`.
  public let settings: String
  /// The session that asked, in a few words, such as "Claude Code in atlas".
  public let session: String
  /// When it must be answered by, after which the command line refuses any answer.
  public let until: Date
  /// What the person answered already; nil while they have not.
  public let answered: LifecycleAnswered?

  private enum Key: String, CodingKey {
    case id
    case kind
    case name
    case vm
    case settings
    case session
    case until
    case answered
  }

  /// Creates a request, as a test or a preview builds one.
  ///
  /// - Parameters:
  ///   - id: The request's ID.
  ///   - kind: What it asks.
  ///   - name: The VM's name.
  ///   - vm: The VM's identifier.
  ///   - settings: The settings file it changes.
  ///   - session: The session that asked.
  ///   - until: When it must be answered by.
  ///   - answered: What the person answered already, or nil.
  public init(
    id: String,
    kind: LifecycleKind,
    name: String,
    vm: String,
    settings: String,
    session: String,
    until: Date,
    answered: LifecycleAnswered? = nil
  ) {
    self.id = id
    self.kind = kind
    self.name = name
    self.vm = vm
    self.settings = settings
    self.session = session
    self.until = until
    self.answered = answered
  }

  /// Read from the command's reply, its deadline in milliseconds since 1970.
  ///
  /// - Parameter decoder: The reply's decoder.
  /// - Throws: The decoding error of a field missing or of the wrong type, or of a kind this app does not know.
  public init(from decoder: any Decoder) throws {
    let fields = try decoder.container(keyedBy: Key.self)
    id = try fields.decode(String.self, forKey: .id)
    kind = try fields.decode(LifecycleKind.self, forKey: .kind)
    name = try fields.decode(String.self, forKey: .name)
    vm = try fields.decode(String.self, forKey: .vm)
    settings = try fields.decode(String.self, forKey: .settings)
    session = try fields.decode(String.self, forKey: .session)
    let milliseconds = try fields.decode(Int64.self, forKey: .until)
    until = Date(timeIntervalSince1970: TimeInterval(milliseconds) / 1000)
    answered = try fields.decodeIfPresent(LifecycleAnswered.self, forKey: .answered)
  }
}
