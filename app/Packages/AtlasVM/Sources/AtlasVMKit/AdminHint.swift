/// What a new VM's setup window says of its second administrator while it
/// takes the person's input: the name and the password to type when macOS
/// asks for an administrator in the VM, the password hidden until the
/// person shows it. Atlas never types it, and never says it anywhere else.
public struct AdminHint: Equatable, Sendable {
  /// The administrator's account name in the guest.
  public static let name = "atlas-admin"
  /// What the window says before the password.
  public static let words = "If macOS asks for an administrator, use \(name) and this password:"
  /// What it says after it: its groups are set apart only to be read.
  public static let typing = "Type it without spaces."
  /// The mark each hidden letter is shown as.
  private static let hidden: Character = "\u{2022}"

  private let password: String
  /// Whether the person shows the password now.
  public private(set) var revealed = false

  /// The hint of a window opened for a new VM's setup, `setup`, whose
  /// administrator's password is `password`, as the VM's creation kept it;
  /// none for any other window, or with no password kept.
  public init?(setup: Bool, password: String?) {
    guard setup, let password, !password.isEmpty else { return nil }
    self.password = password
  }

  /// The password as the window shows it now, in its groups: each letter a
  /// dot until it is revealed.
  public var shown: [String] {
    AdminPassphrase.grouped(password)
      .map { group in revealed ? group : String(repeating: Self.hidden, count: group.count) }
  }

  /// The button's title: what pressing it does.
  public var button: String { revealed ? "Hide" : "Show" }

  /// Show the password, or hide it again.
  public mutating func toggle() {
    revealed.toggle()
  }
}

extension AdminHint: CustomStringConvertible, CustomDebugStringConvertible {
  /// Never the password, so no log or message that names a hint carries it.
  public var description: String { "AdminHint(\(Self.name), revealed: \(revealed))" }

  /// As its description.
  public var debugDescription: String { description }
}
