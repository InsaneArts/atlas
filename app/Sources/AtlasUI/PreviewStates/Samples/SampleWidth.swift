#if DEBUG
  import CoreGraphics

  /// Widths samples are laid out at, so rows show as they do in a window.
  enum SampleWidth {
    /// A setup screen's column: ``OnboardingLayout/column``.
    static let column = OnboardingLayout.column
    /// 300: a side column of cards, and about the panel's content.
    static let narrow: CGFloat = 300
  }
#endif
