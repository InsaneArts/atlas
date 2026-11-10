import SwiftUI

/// A number the person types, or steps by `step`, in a Settings row: it takes effect once they press Return or leave
/// the field, and a number typed outside its range is taken as its nearer end.
struct SettingsNumberField: View {
  /// What the number is, read aloud.
  let label: String
  /// The number.
  @Binding var value: Int
  /// What it may be.
  let range: ClosedRange<Int>
  /// How far a step moves it.
  var step = 1

  /// The field and its stepper.
  var body: some View {
    let within = Binding {
      value
    } set: {
      value = min(max($0, range.lowerBound), range.upperBound)
    }
    HStack(spacing: Spacing.xs) {
      TextField(label, value: within, format: .number.grouping(.never))
        .textFieldStyle(.roundedBorder)
        .multilineTextAlignment(.trailing)
        .font(Typography.digits(.callout))
        .frame(width: SettingsLayout.numberWidth)
      Stepper(label, value: within, in: range, step: step)
        .labelsHidden()
    }
    .fixedSize()
  }
}
