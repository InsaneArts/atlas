import SwiftUI

extension CapsuleButtonStyle {
  /// The label's size: a step larger on a large capsule.
  var textStyle: TextStyle {
    switch size {
    case .mini, .compact, .small, .regular: .body
    case .large: .headline
    }
  }

  /// The label's weight: regular for plain text, semibold on a capsule and on an alert's buttons (v3 `.acap`).
  var weight: Font.Weight {
    switch look {
    case .plain: .regular
    case .primary, .glass, .pressed, .accent, .tinted, .destructive: .semibold
    }
  }

  /// The label's letter spacing: drawn in on a capsule, as the boards set every capsule's word (v3.py `pill`); plain
  /// text and an alert's buttons keep the font's own.
  var tracking: CGFloat {
    switch look {
    case .primary, .glass, .pressed: LetterSpacing.control
    case .plain, .accent, .tinted, .destructive: 0
    }
  }

  /// The capsule's height, and a circle's side.
  var height: CGFloat {
    switch size {
    case .mini: ControlHeight.xsPlus
    case .compact: ControlHeight.s
    case .small: ControlHeight.sPlus
    case .regular: ControlHeight.mPlus
    case .large: ControlHeight.xl
    }
  }

  /// The space on each side of the label: wide for a large capsule, narrow for plain text or a capsule sharing
  /// its row, none on an alert's.
  var inset: CGFloat {
    if size == .large { return Layout.largeInset }
    // An alert's buttons share a narrow row and hold a mark beside their word, so they keep no room of their own
    // beside it, as v3's `.acap` has none.
    if size == .compact { return .zero }
    if look == .plain { return Spacing.m }
    return fill ? Spacing.sPlus : Layout.regularInset
  }

  /// The label's colour.
  var foreground: Color {
    switch look {
    case .primary: .ground
    case .glass, .pressed, .tinted: .textPrimary
    case .plain: .textSecondary
    case .accent: .onAccent
    case .destructive: .textDestructive
    }
  }

  /// The capsule's edge: a glass hairline, a brighter one while pressed, a faint one on an alert's quiet buttons
  /// (v3 `.acap`), none otherwise.
  var edge: Color {
    switch look {
    case .glass: .strokeStrong
    case .pressed: .strokePressed
    case .tinted, .destructive: .separator
    case .primary, .plain, .accent: .clear
    }
  }

  /// Whether the system's glass lies under the fill.
  var isGlazed: Bool {
    backing == .glass && look == .glass
  }
}

/// The capsule's own measures, off the spacing grid's steps.
private enum Layout {
  /// 26: a large capsule's label inset, so a screen's main action reads wider than its text.
  static let largeInset: CGFloat = 26
  /// 18: a regular capsule's label inset when it hugs its label.
  static let regularInset: CGFloat = 18
}
