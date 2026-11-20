import SwiftUI

/// The popup beside ⋯ in the panel's header while more than one desktop is set up: the title of the desktop shown and
/// a chevron, as a small popup button draws them, opening the desktops under it, each with its dot and state, the one
/// shown checked. The shell opens the menu (``PanelEvent/showDesktops(below:)``), so its items carry their dots.
struct PanelDesktopButton: View {
  /// The popup.
  let picker: PanelDesktopPicker
  /// Receives the click, with the popup's frame.
  let send: @MainActor (PanelEvent) -> Void
  /// Where the popup is in the panel, so its menu opens under it.
  @State private var frame = CGRect.zero

  /// The popup, pressed while its menu is open.
  var body: some View {
    Button {
      send(.showDesktops(below: frame))
    } label: {
      PopupButtonLabel(title: picker.title, isOn: picker.isOpen)
    }
    .buttonStyle(.plain)
    .fixedSize()
    .onGeometryChange(for: CGRect.self) {
      $0.frame(in: .global)
    } action: {
      frame = $0
    }
    .help("Choose a desktop")
    .accessibilityLabel("Desktop")
    .accessibilityValue(picker.title)
    .accessibilityHint("Opens a menu")
  }
}
