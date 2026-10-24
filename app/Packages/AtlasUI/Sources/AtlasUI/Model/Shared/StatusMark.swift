/// The mark beside a status's word, which its tone paints.
public enum StatusMark: Sendable {
  /// A steady dot, which glows unless its tone is quiet.
  case dot
  /// A dot that breathes while something works, steady under Reduce Motion.
  case pulsing
  /// A rounded square, a stop button's shape: the person stopped agents.
  case square
  /// A smaller dot with no glow: nothing can happen yet.
  case small
}
