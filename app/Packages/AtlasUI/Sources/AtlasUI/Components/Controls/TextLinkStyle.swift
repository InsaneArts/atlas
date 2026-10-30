import SwiftUI

/// A link's look for a button among a row's words, such as Open or Show on screen: link-blue small text, dimmed while
/// pressed, at once under Reduce Motion. A style, not a view, so the control stays a `Button` with its own label, help and disabled state.
struct TextLinkStyle: ButtonStyle {
  /// A pressed link dims like a link in a Mac app.
  private static let pressedOpacity = 0.6
  /// How large the link's words are.
  enum Size: Sendable {
    /// 12, regular: among a row's words.
    case regular
    /// 11, medium: beside a group's label, such as the panel's All activity.
    case small
  }

  /// How large the link's words are.
  var size = Size.regular
  @Environment(\.isEnabled) private var enabled
  @Environment(\.motionReduced) private var reduceMotion

  /// Draws the label as a link.
  ///
  /// - Parameter configuration: The button's label and pressed state.
  /// - Returns: The styled button.
  func makeBody(configuration: Configuration) -> some View {
    configuration.label
      .font(font)
      .foregroundStyle(Color.textLink)
      .lineLimit(1)
      .contentShape(Rectangle())
      .opacity(configuration.isPressed ? Self.pressedOpacity : 1)
      .opacity(enabled ? 1 : Opacity.disabled)
      .animation(Motion.animation(Motion.press, reduceMotion: reduceMotion), value: configuration.isPressed)
  }

  /// The words' font for the size.
  private var font: Font {
    switch size {
    case .regular: Typography.font(.callout)
    case .small: Typography.font(.caption, weight: .medium)
    }
  }
}

#if DEBUG
  #Preview("Samples") {
    HStack(spacing: Spacing.mPlus) {
      Button(String("Open")) {}
        .buttonStyle(.textLink)
      Button {
      } label: {
        Label(String("Show on screen"), systemImage: "eye")
      }
      .buttonStyle(.textLink)
    }
    .sampleSurface()
  }
#endif
