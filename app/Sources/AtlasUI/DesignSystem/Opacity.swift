/// Opacities applied at draw time, over a view or a token colour.
enum Opacity {
  /// 0.35: a disabled control.
  static let disabled = 0.35
  /// 0.45: a paused picture, dimmed so it reads as not live.
  static let paused = 0.45
  /// 0.5: a form while its command runs.
  static let busy = 0.5
  /// 0.12: a tinted fill, such as a selected row's or a taught project's.
  static let tint = 0.12
  /// 0.16: a tag's tinted capsule, such as In front.
  static let tag = 0.16
  /// 0.55: a tinted fill's edge.
  static let tintEdge = 0.55
  /// 0.3: a quieter tinted edge, such as an attention card's.
  static let tintEdgeQuiet = 0.3
}
