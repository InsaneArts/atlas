import SwiftUI

/// A switch tinted in the accent, its label hidden: the row beside it names it.
struct AccentSwitchStyle: ToggleStyle {
  /// Draws the toggle as a switch.
  ///
  /// - Parameter configuration: The toggle's label and state.
  /// - Returns: The switch.
  func makeBody(configuration: Configuration) -> some View {
    Toggle(configuration)
      .toggleStyle(.switch)
      .labelsHidden()
      .tint(Color.accent)
  }
}

#if DEBUG
  #Preview("Samples") {
    VStack(spacing: Spacing.l) {
      Toggle(String("On"), isOn: .constant(true))
      Toggle(String("Off"), isOn: .constant(false))
      Toggle(String("Mini"), isOn: .constant(true))
        .controlSize(.mini)
      Toggle(String("Disabled"), isOn: .constant(true))
        .disabled(true)
    }
    .toggleStyle(.accentSwitch)
    .sampleSurface()
  }

  #Preview("Interactive") {
    @Previewable @State var isOn = false
    InteractivePreview {
      Toggle(String("Open at Login"), isOn: $isOn)
        .toggleStyle(.accentSwitch)
    }
  }
#endif
