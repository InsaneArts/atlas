import SwiftUI

/// A menu's content drawn from data, for a context menu or a menu a click opens: each section under its heading, a
/// separator between sections, a ticked item as a toggle so the menu draws its check, an item with a submenu as one, a
/// fact as a line that cannot be chosen, a destructive item drawn as such, and each item's shortcut beside it.
/// Choosing an item hands its command back; an item that cannot be chosen now is drawn disabled. Empty sections draw
/// nothing, so an empty menu offers nothing.
struct CommandMenu: View {
  /// The menu's sections, in order.
  let sections: [ViewerMenuSection]
  /// Receives the command chosen.
  let choose: @MainActor (ViewerCommand) -> Void

  /// The sections, apart.
  var body: some View {
    CommandMenuSections(sections: sections) { item in
      entry(item)
    }
  }

  /// One item: a submenu of its sections, a fact, a ticked toggle, or a button.
  ///
  /// - Parameter item: The item.
  /// - Returns: Its entry.
  @ViewBuilder private func entry(_ item: ViewerMenuItem) -> some View {
    if !item.submenu.isEmpty {
      Menu(item.title) {
        CommandMenuSections(sections: item.submenu) { child in
          leaf(child)
        }
      }
      .disabled(!item.isEnabled)
    } else {
      leaf(item)
    }
  }

  /// An item with no submenu: a fact as a line that cannot be chosen, a toggle while ticked, so the menu draws its
  /// check, else a button, in its role, with its shortcut.
  ///
  /// - Parameter item: The item.
  /// - Returns: Its entry.
  @ViewBuilder private func leaf(_ item: ViewerMenuItem) -> some View {
    if item.isInfo {
      Text(item.title)
    } else if item.isOn {
      Toggle(
        item.title,
        isOn: Binding {
          true
        } set: { _ in
          send(item)
        }
      )
      .disabled(!item.isEnabled)
    } else {
      Button(item.title, role: item.role == .destructive ? .destructive : nil) { send(item) }
        .keyboardShortcut(Self.shortcut(item.shortcut))
        .disabled(!item.isEnabled || item.command == nil)
    }
  }

  /// Hands an item's command back, if it has one.
  ///
  /// - Parameter item: The item chosen.
  private func send(_ item: ViewerMenuItem) {
    if let command = item.command { choose(command) }
  }

  /// The shortcut a menu draws beside an item, with Command and the modifiers it names.
  ///
  /// - Parameter shortcut: The item's shortcut.
  /// - Returns: SwiftUI's shortcut, or nil for none.
  nonisolated static func shortcut(_ shortcut: ViewerMenuShortcut?) -> KeyboardShortcut? {
    guard let shortcut else { return nil }
    var modifiers = EventModifiers.command
    if shortcut.option { modifiers.insert(.option) }
    if shortcut.control { modifiers.insert(.control) }
    if shortcut.shift { modifiers.insert(.shift) }
    return KeyboardShortcut(KeyEquivalent(shortcut.key), modifiers: modifiers)
  }
}

#if DEBUG
  #Preview("Right-click the square") {
    RoundedRectangle(cornerRadius: Radius.medium, style: .continuous)
      .fill(Color.fillControl)
      .frame(width: SampleWidth.column, height: SampleWidth.column)
      .contextMenu {
        CommandMenu(sections: PreviewStates.viewerSessionMenu) { _ in }
      }
      .padding(Spacing.l)
      .sampleSurface()
  }
#endif
