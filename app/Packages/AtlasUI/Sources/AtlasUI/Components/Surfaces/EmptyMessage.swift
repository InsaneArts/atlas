import SwiftUI

/// Why there is nothing to show, on the empty stage: a quiet orbit, a headline, a sentence and, at most, one button.
/// It reads as one unless it offers a button to reach. On the main window's stage it takes v3's waiting pattern (V7):
/// the mark at 64, a 17-point title, one line and one button, the mark ringed while something connects.
struct EmptyMessage: View {
  /// How large the message is drawn.
  enum Size: Sendable {
    /// In a pane: a 30 orbit, a 15 headline and a 13 sentence.
    case regular
    /// In a small window: a 26 orbit, a 14 headline and a 12 sentence.
    case small
    /// On the main window's stage, waiting on the helper (v3 V7): a 64 mark, a 17 title, a 13 line and a 34 button.
    case stage
  }

  /// What the message stands on.
  enum Ground: Sendable {
    /// The empty stage, in place of a picture.
    case stage
    /// Nothing of its own: a pane's card, or a stage drawn by its owner, shows through.
    case pane
  }

  /// What the message says.
  let state: EmptyMessageState
  /// How large it is drawn.
  var size = Size.regular
  /// What it stands on: the empty stage by default.
  var ground = Ground.stage
  /// Whether a ring traces round the mark, while something connects.
  var tracesMark = false
  /// Receives a press of its button.
  var act: @MainActor () -> Void = {}

  /// The mark, the headline, the sentence and the button, on the stage or on nothing.
  var body: some View {
    VStack(spacing: .zero) {
      mark
        .padding(.bottom, markGap)
      Text(state.headline)
        .font(Typography.font(headline, weight: .semibold))
      if !state.message.isEmpty {
        Text(state.message)
          .font(Typography.font(sentence))
          .foregroundStyle(Color.textSecondary)
          .frame(maxWidth: width)
          .padding(.top, lineGap)
      }
      if let action = state.action {
        Button(action.title, action: act)
          .buttonStyle(.capsule(action.isPrimary ? .primary : .glass, size: buttonSize))
          .padding(.top, markGap)
      }
    }
    .multilineTextAlignment(.center)
    .fixedSize(horizontal: false, vertical: true)
    .padding(Spacing.xl)
    .frame(maxWidth: .infinity, maxHeight: .infinity)
    .background {
      if ground == .stage {
        RoundedRectangle(cornerRadius: corner)
          .filledOutline(Color.fillPlaceholder, border: Color.separator)
      }
    }
    .accessibilityElement(children: children)
  }

  /// The orbit, quiet in a pane, whole on the stage with its ring while something connects.
  @ViewBuilder private var mark: some View {
    switch size {
    case .regular, .small:
      OrbitMark(height: orbit)
        .foregroundStyle(Color.textTertiary)
    case .stage:
      OrbitMark(size: IconSize.xxl)
        .foregroundStyle(Color.textPrimary)
        .overlay {
          if tracesMark {
            OrbitRing()
              .transition(.opacity)
          }
        }
        .animation(Motion.fade, value: tracesMark)
    }
  }

  /// How VoiceOver reads it: as one, unless it holds a button to reach.
  private var children: AccessibilityChildBehavior {
    state.action == nil ? .combine : .contain
  }

  /// The orbit's height.
  private var orbit: CGFloat {
    switch size {
    case .regular, .stage: Layout.orbit
    case .small: Layout.smallOrbit
    }
  }

  /// Under the mark, and over the button.
  private var markGap: CGFloat {
    switch size {
    case .regular: Spacing.mPlus
    case .small: Spacing.m
    case .stage: Layout.stageGap
    }
  }

  /// Between the headline and the sentence.
  private var lineGap: CGFloat {
    switch size {
    case .regular: Spacing.mPlus
    case .small: Spacing.m
    case .stage: Spacing.xsPlus
    }
  }

  /// The headline's size.
  private var headline: TextStyle {
    switch size {
    case .regular: .heading
    case .small: .headline
    case .stage: .subheading
    }
  }

  /// The sentence's size.
  private var sentence: TextStyle {
    switch size {
    case .regular, .stage: .body
    case .small: .callout
    }
  }

  /// The widest the sentence runs before it wraps.
  private var width: CGFloat {
    switch size {
    case .regular: Layout.width
    case .small: Layout.smallWidth
    case .stage: Layout.stageWidth
    }
  }

  /// The button's size.
  private var buttonSize: CapsuleButtonStyle.Size {
    size == .stage ? .regular : .small
  }

  /// The stage's corners.
  private var corner: CGFloat {
    switch size {
    case .regular, .stage: Radius.large
    case .small: Radius.medium
    }
  }
}

/// The message's own measures, from the boards (mac.py `empty`, MacEmpty's small windows) and v3 (`.vempty`).
private enum Layout {
  /// 30: the orbit's height in the main window; only an empty message draws an orbit this large.
  static let orbit: CGFloat = 30
  /// 26: the orbit's height in a small window.
  static let smallOrbit: CGFloat = 26
  /// 380: the widest the sentence runs in the main window.
  static let width: CGFloat = 380
  /// 360: the widest it runs in a small window.
  static let smallWidth: CGFloat = 360
  /// 340: the widest the line runs on the stage (v3 `.vempty p`).
  static let stageWidth: CGFloat = 340
  /// 18: under the mark and over the button on the stage (v3 `.vempty .em`, `.cap`).
  static let stageGap: CGFloat = 18
}

#if DEBUG
  #Preview("Samples") {
    EmptyMessageSamples()
  }
#endif
