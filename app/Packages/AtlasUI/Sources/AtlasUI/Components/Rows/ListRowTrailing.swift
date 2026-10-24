import SwiftUI

/// What ends a ``ListRow``: a quiet word, a mark and a word, a switch, a button, a radio or a spinner.
struct ListRowTrailing: View {
  /// What ends the row.
  let trailing: ListRow.Trailing
  /// What its switch or button does.
  let action: (@MainActor () -> Void)?

  /// The trailing control, or nothing.
  var body: some View {
    switch trailing {
    case .none:
      EmptyView()
    case .text(let text, let tone):
      Text(text)
        .textStyle(.callout)
        .foregroundStyle(tone.color)
        .lineLimit(1)
    case .check(let state, let word):
      HStack(spacing: Spacing.xs) {
        OutcomeMark(kind: StatusLine.Kind(state))
        Text(word)
          .textStyle(.callout, weight: .medium)
          .foregroundStyle(StatusLine.color(StatusLine.Kind(state)))
          .lineLimit(1)
      }
    case .toggle(let isOn):
      Toggle(String(), isOn: switchBinding(isOn))
        .toggleStyle(.accentSwitch)
        .labelsHidden()
    case .button(let title):
      Button(title) { action?() }
        .buttonStyle(.capsule(.glass, size: .mini))
    case .radio(let isOn):
      RadioMark(isOn: isOn)
    case .progress:
      Spinner()
        .appearsAfterDelay()
    }
  }

  /// The switch's state, which only the row's owner changes: flipping it calls the action, and the next state draws
  /// the new value.
  ///
  /// - Parameter isOn: Whether the switch is on now.
  /// - Returns: A binding that reads `isOn` and calls the action on a flip.
  private func switchBinding(_ isOn: Bool) -> Binding<Bool> {
    Binding(get: { isOn }, set: { _ in action?() })
  }
}
