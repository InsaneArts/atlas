/// Opacities applied at draw time, over a view or a token colour, faintest first.
enum Opacity {
  /// 0.12: a tinted fill, such as a selected row's or a taught project's.
  static let tint = 0.12
  /// 0.35: a disabled control.
  static let disabled = 0.35
  /// 0.5: a muted form, such as one whose command runs.
  static let muted = 0.5
  /// 0.6: a step whose turn has not come yet, such as the second permission while the first is allowed.
  static let notYet = 0.6
}
