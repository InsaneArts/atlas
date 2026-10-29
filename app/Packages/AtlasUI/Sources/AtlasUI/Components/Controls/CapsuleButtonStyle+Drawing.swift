import SwiftUI

extension CapsuleButtonStyle {
  /// The label's size: a step larger on a large capsule.
  var textStyle: TextStyle {
    switch size {
    case .mini, .compact, .small, .regular: .body
    case .large: .headline
    }
  }

  /// The label's weight: regular for plain text, semibold on a capsule and on an alert's buttons.
  var weight: Font.Weight {
    switch look {
    case .plain: .regular
    case .primary, .secondary, .pressed, .accent, .tinted, .destructive, .glyph, .overPicture: .semibold
    }
  }

  /// The label's letter spacing: drawn in on a capsule, to keep short control labels compact; plain text and
  /// an alert's buttons keep the font's own.
  var tracking: CGFloat {
    switch look {
    case .primary, .secondary, .pressed, .overPicture: LetterSpacing.control
    case .plain, .accent, .tinted, .destructive, .glyph: 0
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
    // An alert's buttons share a narrow row and hold a mark beside their word. No extra space is reserved
    // beside the mark, so the button's label stays centred.
    if size == .compact { return .zero }
    if look == .plain { return Spacing.m }
    return fill ? Spacing.sPlus : Layout.regularInset
  }

  /// The space before the label: a symbol pill's 12, else ``inset``.
  var leadingInset: CGFloat {
    outline == .symbolPill ? Spacing.m : inset
  }

  /// The space after the label: a symbol pill's 14, else ``inset``.
  var trailingInset: CGFloat {
    outline == .symbolPill ? Spacing.mPlus : inset
  }

  /// The label's colour.
  var foreground: Color {
    switch look {
    case .primary: .ground
    case .secondary, .pressed, .tinted: .textPrimary
    case .plain: .textSecondary
    case .accent: .onAccent
    case .destructive: .textDestructive
    case .glyph: isOn ? .textPrimary : .textSecondary
    case .overPicture: .textOverPicture
    }
  }
}

/// The capsule's own measures, off the spacing grid's steps.
private enum Layout {
  /// 26: a large capsule's label inset, so a screen's main action reads wider than its text.
  static let largeInset: CGFloat = 26
  /// 18: a regular capsule's label inset when it hugs its label.
  static let regularInset: CGFloat = 18
}
