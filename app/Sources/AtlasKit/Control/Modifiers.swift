/// The modifier keys held, as the service names them, in its order.
package struct Modifiers: OptionSet, Sendable, Hashable {
  /// The set's bits.
  package let rawValue: Int

  /// Creates a set from its bits.
  ///
  /// - Parameter rawValue: The set's bits.
  package init(rawValue: Int) { self.rawValue = rawValue }

  /// Command.
  package static let command = Modifiers(rawValue: 1)
  /// Control.
  package static let control = Modifiers(rawValue: 2)
  /// Option.
  package static let option = Modifiers(rawValue: 4)
  /// Shift.
  package static let shift = Modifiers(rawValue: 8)

  /// The keys held, as the service names them, in its order: `cmd`, `ctrl`, `alt`, `shift`.
  package var names: [String] {
    [(Modifiers.command, "cmd"), (.control, "ctrl"), (.option, "alt"), (.shift, "shift")]
      .filter { contains($0.0) }
      .map(\.1)
  }
}
