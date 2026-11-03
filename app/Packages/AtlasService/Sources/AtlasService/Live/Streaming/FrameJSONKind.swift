/// What a metadata value is, as the lexical pass read it.
enum FrameJSONKind: Sendable, Equatable {
  /// A number in plain decimal integer syntax: no sign, fraction or exponent.
  case integer
  /// Any other number.
  case number
  /// A string.
  case string
  /// `true` or `false`.
  case bool
  /// `null`.
  case null
  /// An object.
  case object
  /// An array.
  case array
}
