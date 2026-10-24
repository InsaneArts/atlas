#if DEBUG
  import CoreGraphics

  /// Widths samples are laid out at, so rows show as they do in a window.
  enum SampleWidth {
    /// A setup screen's column: ``OnboardingLayout/column``.
    static let column = OnboardingLayout.column
    /// 228: an alert's buttons, inside its 260 less its 16 on each side.
    static let alert: CGFloat = 228
    /// 300: a side column of cards, and about the panel's content.
    static let narrow: CGFloat = 300
    /// A settings form: ``SettingsLayout/formWidth``.
    static let form = SettingsLayout.formWidth
  }
#endif
