import SwiftUI

/// A small popup button, as a settings row ends with one: the choice's word and a chevron on a glass fill, opening
/// the choices as a menu with the current one checked.
struct PopupButton<Value: Hashable & Sendable>: View {
  /// The popup's name, which VoiceOver reads, such as "Picture".
  let label: String
  /// The choices, in menu order.
  let options: [PopupOption<Value>]
  /// The current choice.
  @Binding var selection: Value

  var body: some View {
    Menu {
      Picker(label, selection: $selection) {
        ForEach(options) { option in
          Text(option.title)
            .tag(option.id)
            .disabled(!option.isEnabled)
        }
      }
      .pickerStyle(.inline)
      .labelsHidden()
    } label: {
      HStack(spacing: Spacing.s) {
        Text(options.title(of: selection))
          .font(Typography.font(.callout))
        Image(systemName: "chevron.down")
          .font(Typography.font(.footnote, weight: .semibold))
          .accessibilityHidden(true)
      }
      .foregroundStyle(Color.textPrimary)
      .padding(.leading, Spacing.ml)
      .padding(.trailing, Spacing.m)
      .frame(height: ControlHeight.pill)
      .background(
        RoundedRectangle(cornerRadius: Radius.compact)
          .fill(Color.fillControl)
      )
      .overlay(
        RoundedRectangle(cornerRadius: Radius.compact)
          .strokeBorder(Color.strokeStrong, lineWidth: Stroke.hairline)
      )
      .contentShape(RoundedRectangle(cornerRadius: Radius.compact))
    }
    .menuStyle(.button)
    .buttonStyle(.plain)
    .menuIndicator(.hidden)
    .fixedSize()
    .accessibilityLabel(label)
    .accessibilityValue(options.title(of: selection))
  }
}

#if DEBUG
  #Preview("Samples") {
    PopupButtonSamples()
  }

  #Preview("Interactive") {
    @Previewable @State var selection = PopupButtonSamples.Rate.live
    InteractivePreview {
      PopupButton(label: String("Picture"), options: PopupButtonSamples.options, selection: $selection)
    }
  }
#endif
