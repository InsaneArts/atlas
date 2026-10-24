import SwiftUI

/// Why there is nothing to show, on the empty stage: a quiet orbit, a headline, a sentence and, at most, one button.
/// It reads as one unless it offers a button to reach.
struct EmptyMessage: View {
  /// How large the message is drawn.
  enum Size: Sendable {
    /// In the main window: a 30 orbit, a 15 headline and a 13 sentence.
    case regular
    /// In a small window: a 26 orbit, a 14 headline and a 12 sentence.
    case small
  }

  /// What the message stands on.
  enum Ground: Sendable {
    /// The empty stage, in place of a picture.
    case stage
    /// Nothing of its own: a pane's card shows through, in place of a list.
    case pane
  }

  /// What the message says.
  let state: EmptyMessageState
  /// How large it is drawn.
  var size = Size.regular
  /// What it stands on: the empty stage by default.
  var ground = Ground.stage
  /// Receives a press of its button.
  var act: @MainActor () -> Void = {}

  var body: some View {
    VStack(spacing: gap) {
      OrbitMark(height: orbit)
        .foregroundStyle(Color.textTertiary)
      Text(state.headline)
        .font(Typography.font(headline, weight: .semibold))
      Text(state.message)
        .font(Typography.font(sentence))
        .foregroundStyle(Color.textSecondary)
        .frame(maxWidth: width)
      if let action = state.action {
        Button(action.title, action: act)
          .buttonStyle(.capsule(action.isPrimary ? .primary : .glass, size: .small))
      }
    }
    .multilineTextAlignment(.center)
    .fixedSize(horizontal: false, vertical: true)
    .padding(Spacing.xl)
    .frame(maxWidth: .infinity, maxHeight: .infinity)
    .background {
      if ground == .stage {
        RoundedRectangle(cornerRadius: corner)
          .fill(Color.fillPlaceholder)
        RoundedRectangle(cornerRadius: corner)
          .strokeBorder(Color.separator, lineWidth: Stroke.hairline)
      }
    }
    .accessibilityElement(children: children)
  }

  /// How VoiceOver reads it: as one, unless it holds a button to reach.
  private var children: AccessibilityChildBehavior {
    state.action == nil ? .combine : .contain
  }

  /// The orbit's height.
  private var orbit: CGFloat {
    switch size {
    case .regular: Layout.orbit
    case .small: Layout.smallOrbit
    }
  }

  /// Between the parts.
  private var gap: CGFloat {
    switch size {
    case .regular: Spacing.mPlus
    case .small: Spacing.m
    }
  }

  /// The headline's size.
  private var headline: TextStyle {
    switch size {
    case .regular: .heading
    case .small: .headline
    }
  }

  /// The sentence's size.
  private var sentence: TextStyle {
    switch size {
    case .regular: .body
    case .small: .callout
    }
  }

  /// The widest the sentence runs before it wraps.
  private var width: CGFloat {
    switch size {
    case .regular: Layout.width
    case .small: Layout.smallWidth
    }
  }

  /// The stage's corners.
  private var corner: CGFloat {
    switch size {
    case .regular: Radius.large
    case .small: Radius.medium
    }
  }
}

/// The message's own measures, from the boards (mac.py `empty`, MacEmpty's small windows).
private enum Layout {
  /// 30: the orbit's height in the main window; only an empty message draws an orbit this large.
  static let orbit: CGFloat = 30
  /// 26: the orbit's height in a small window.
  static let smallOrbit: CGFloat = 26
  /// 380: the widest the sentence runs in the main window.
  static let width: CGFloat = 380
  /// 360: the widest it runs in a small window.
  static let smallWidth: CGFloat = 360
}

#if DEBUG
  #Preview("Samples") {
    EmptyMessageSamples()
  }
#endif
