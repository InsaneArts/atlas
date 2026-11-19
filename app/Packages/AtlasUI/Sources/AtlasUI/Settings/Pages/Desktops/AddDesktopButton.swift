import SwiftUI

/// Add a Desktop… at a page's trailing edge, under its last group, as Accounts sets its add buttons: it opens the
/// setup window on its choice of a desktop.
struct AddDesktopButton: View {
  /// Receives what the person does.
  let send: @MainActor (SettingsEvent) -> Void

  /// The button, inset to where the rows' controls end.
  var body: some View {
    Button("Add a Desktop…") { send(.addDesktop) }
      .buttonStyle(.capsule(.secondary, size: .row))
      .padding(.trailing, ListRow.Size.compact.inset)
      .frame(maxWidth: .infinity, alignment: .trailing)
  }
}
