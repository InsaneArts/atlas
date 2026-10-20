import SwiftUI

/// A setting with its switch: its title and what it means on the left, the switch on the right.
struct SettingRow: View {
  /// The setting's name, which also labels the switch.
  let title: String
  /// What it means.
  let detail: String
  /// Whether it is on.
  @Binding var isOn: Bool

  var body: some View {
    ControlRow(size: .tall) {
      TitleDetail(title: title, detail: detail)
    } control: {
      Toggle(title, isOn: $isOn)
        .toggleStyle(.accentSwitch)
    }
  }
}

#if DEBUG
  #Preview("Samples") {
    RowSamples(.setting)
  }
#endif
