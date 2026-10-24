/// Opacities applied at draw time, over a view or a token colour, faintest first.
enum Opacity {
  /// 0.12: a tinted fill, such as a selected row's or a taught project's.
  static let tint = 0.12
  /// 0.16: a stronger tinted fill, such as a tag's capsule (In front).
  static let tintStrong = 0.16
  /// 0.3: a quieter tinted edge, such as an attention card's.
  static let edgeTintQuiet = 0.3
  /// 0.35: a disabled control.
  static let disabled = 0.35
  /// 0.45: a dimmed picture, such as a paused one, so it reads as not live.
  static let dimmed = 0.45
  /// 0.5: a muted form, such as one whose command runs.
  static let muted = 0.5
  /// 0.55: a tinted fill's edge.
  static let edgeTint = 0.55
  /// 0.6: a step whose turn has not come yet, such as the second permission while the first is allowed.
  static let notYet = 0.6
}
