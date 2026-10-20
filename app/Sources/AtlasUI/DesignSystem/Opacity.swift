/// Opacities applied at draw time, over a view or a token colour.
enum Opacity {
  /// 0.35: a disabled control.
  static let disabled = 0.35
  /// 0.5: a form while its command runs.
  static let busy = 0.5
  /// 0.12: a tinted fill, such as a selected row's or a taught project's.
  static let tint = 0.12
  /// 0.55: a tinted fill's edge.
  static let tintEdge = 0.55
  /// 0.35: a quieter tinted edge, such as an attention card's.
  static let tintEdgeQuiet = 0.35
}
