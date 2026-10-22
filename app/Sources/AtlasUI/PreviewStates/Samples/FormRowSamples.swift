#if DEBUG
  import SwiftUI

  /// The form row's samples: a switch, a status's dot with a button, a check, and an agent's tile with its word.
  struct FormRowSamples: View {
    var body: some View {
      CardGroup(radius: Radius.medium) {
        FormRow(title: String("Open Atlas at login"), detail: String("Atlas sits in the menu bar from the start")) {
          Toggle(String("Open Atlas at login"), isOn: .constant(true))
            .toggleStyle(.accentSwitch)
        }
        SeparatorLine()
        FormRow(
          title: String("Atlas"),
          detail: String("The hidden helper login · 2560 × 1440"),
          accessory: .status(.dot, .quiet)
        ) {
          Button(String("Switch to the Helper…")) {}
            .buttonStyle(.capsule(.glass, size: .mini))
        }
        SeparatorLine()
        FormRow(title: String("Mouse and keyboard"), accessory: .readiness(.ready))
        SeparatorLine()
        FormRow(
          title: String("Codex"),
          detail: String("Uses Atlas in every project"),
          accessory: .icon(.tile(.text(">_")))
        ) {
          Text(verbatim: "Connected")
            .font(Typography.font(.callout, weight: .medium))
            .foregroundStyle(Color.success)
        }
      }
      .frame(width: SampleWidth.form)
      .sampleSurface()
    }
  }
#endif
