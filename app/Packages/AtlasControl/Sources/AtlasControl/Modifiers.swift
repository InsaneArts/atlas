/// The modifier keys held, as the service names them, in its order.
public struct Modifiers: OptionSet, Sendable, Hashable {
  /// The set's bits.
  public let rawValue: Int

  /// Creates a set from its bits.
  ///
  /// - Parameter rawValue: The set's bits.
  public init(rawValue: Int) { self.rawValue = rawValue }

  /// Command.
  public static let command = Modifiers(rawValue: 1)
  /// Control.
  public static let control = Modifiers(rawValue: 2)
  /// Option.
  public static let option = Modifiers(rawValue: 4)
  /// Shift.
  public static let shift = Modifiers(rawValue: 8)

  /// The keys held, as the service names them, in its order: `cmd`, `ctrl`, `alt`, `shift`.
  var names: [String] {
    [(Modifiers.command, "cmd"), (.control, "ctrl"), (.option, "alt"), (.shift, "shift")]
      .filter { contains($0.0) }
      .map(\.1)
  }
}
