import SwiftUI

/// A whole row as a button, such as one of the panel's sessions: a faint fill under the pointer and a stronger one
/// while pressed, on small corners, with no scale, so the row stays where it is; at once under Reduce Motion. A style,
/// not a view, so the control stays a `Button` with its own label, help and accessibility.
struct RowButtonStyle: ButtonStyle {
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// Draws the label on its fill.
  ///
  /// - Parameter configuration: The button's label and pressed state.
  /// - Returns: The styled button.
  func makeBody(configuration: Configuration) -> some View {
    RowButtonBody(configuration: configuration, reduceMotion: reduceMotion)
  }
}

#if DEBUG
  #Preview("Samples") {
    VStack(spacing: Spacing.half) {
      ForEach(PreviewStates.panelSessions.sessions) { entry in
        Button {
        } label: {
          Text(entry.name)
            .frame(maxWidth: .infinity, minHeight: PanelLayout.sessionRow, alignment: .leading)
            .padding(.horizontal, Spacing.s)
        }
        .buttonStyle(.row)
      }
    }
    .frame(width: PanelLayout.picture.width)
    .sampleSurface()
  }
#endif
