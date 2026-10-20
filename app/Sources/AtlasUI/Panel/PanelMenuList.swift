import SwiftUI

/// The rows under the panel's cards, set like a menu's items.
struct PanelMenuList: View {
  /// Which rows show, and which can be chosen.
  let menu: PanelMenu
  /// Receives the rows' events.
  let send: @MainActor (PanelEvent) -> Void

  /// Open at Login, Getting Started…, Recover, Log Out Helper… and Quit Atlas.
  var body: some View {
    VStack(spacing: Spacing.hairline) {
      if let isOn = menu.opensAtLogin {
        ControlRow(size: .menu) {
          Text("Open at Login")
            .font(Typography.font(.body))
        } control: {
          Toggle("Open at Login", isOn: Binding(get: { isOn }, set: { send(.setOpenAtLogin($0)) }))
            .toggleStyle(.accentSwitch)
            .controlSize(.mini)
        }
      }
      Button("Getting Started…") { send(.gettingStarted) }
        .buttonStyle(.menuRow())
      if menu.recover != .hidden {
        Button("Recover") { send(.recover) }
          .buttonStyle(.menuRow())
          .disabled(menu.recover == .disabled)
      }
      if menu.logOut != .hidden {
        Button("Log Out Helper…") { send(.askToLogOut) }
          .buttonStyle(.menuRow())
          .disabled(menu.logOut == .disabled)
      }
      Button("Quit Atlas") { send(.quit) }
        .buttonStyle(.menuRow(shortcut: Self.quit))
        .keyboardShortcut(Self.quit)
    }
    .padding(.horizontal, PanelLayout.menuOutset)
  }

  /// ⌘Q, which Quit Atlas answers and writes beside its title.
  private static let quit = KeyboardShortcut("q", modifiers: .command)
}

#if DEBUG
  #Preview("Everything") {
    PanelMenuList(menu: PanelMenu(opensAtLogin: true, recover: .enabled, logOut: .disabled)) { _ in }
      .frame(width: SampleWidth.narrow)
      .sampleSurface()
  }
#endif
