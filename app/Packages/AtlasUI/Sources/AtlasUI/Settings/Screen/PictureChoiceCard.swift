import SwiftUI

/// A single cadence choice, with text, radio selection, keyboard focus and native disabled semantics.
struct PictureChoiceCard: View {
  /// The existing option and its availability.
  let option: PopupOption<SettingsPicture>
  /// Whether this option is the current preference.
  let selected: Bool
  /// Chooses this option through the shared writer.
  let choose: @MainActor () -> Void
  /// Whether the pointer is over this choice.
  @State private var hovered = false
  /// Whether the choice has keyboard focus.
  @FocusState private var focused: Bool

  /// A static illustration above the label and explanation.
  var body: some View {
    Button(action: choose) {
      VStack(alignment: .leading, spacing: Spacing.s) {
        HStack {
          Image(systemName: symbol.rawValue)
            .font(Typography.symbol(.control))
            .frame(width: IconSize.m, height: IconSize.m)
            .accessibilityHidden(true)
          Spacer(minLength: .zero)
          RadioMark(isOn: selected)
        }
        Text(option.title)
          .textStyle(.body)
        Text(explanation)
          .textStyle(.caption)
          .foregroundStyle(Color.textSecondary)
          .fixedSize(horizontal: false, vertical: true)
      }
      .foregroundStyle(Color.textPrimary)
      .padding(Spacing.m)
      .frame(
        minWidth: SettingsLayout.pictureChoiceWidth,
        maxWidth: .infinity,
        minHeight: SettingsLayout.pictureChoiceHeight,
        maxHeight: .infinity,
        alignment: .topLeading
      )
      .background(
        RoundedRectangle(cornerRadius: Radius.small)
          .fill(selected ? Color.accentFill : hovered ? Color.fillControlHover : Color.fillControl)
      )
      .overlay {
        RoundedRectangle(cornerRadius: Radius.small)
          .strokeBorder(selected || focused ? Color.strokeFocus : Color.clear, lineWidth: Stroke.regular)
      }
      .contentShape(RoundedRectangle(cornerRadius: Radius.small))
    }
    .buttonStyle(PictureChoiceStyle())
    .focused($focused)
    .focusEffectDisabled()
    .disabled(!option.isEnabled)
    .onHover { hovered = $0 }
    .animation(Motion.hover(hovered), value: hovered)
    .accessibilityElement(children: .combine)
    .accessibilityAddTraits(selected ? .isSelected : [])
  }

  /// A fixed cadence illustration, never live screen content.
  private var symbol: SymbolName {
    switch option.id {
    case .live: .live
    case .everyFiveSeconds: .clock
    case .paused: .pause
    }
  }

  /// Each choice's scope, including the reason Paused is unavailable.
  private var explanation: String {
    switch option.id {
    case .live: "Show the live picture."
    case .everyFiveSeconds: "Update every five seconds."
    case .paused:
      option.isEnabled ? "Pause the main window's picture." : "Open the main window to pause its picture."
    }
  }
}
