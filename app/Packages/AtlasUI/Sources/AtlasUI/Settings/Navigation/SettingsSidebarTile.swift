import SwiftUI

/// A constant category tile; live helper state is shown separately in text.
struct SettingsSidebarTile: View {
  /// The existing pane represented by the tile.
  let pane: SettingsRoot

  /// The symbol on its category fill.
  var body: some View {
    Image(systemName: symbol.rawValue)
      .font(Typography.symbol(.control))
      .foregroundStyle(foreground)
      .frame(width: IconSize.m, height: IconSize.m)
      .background(RoundedRectangle(cornerRadius: Radius.extraSmall).fill(fill))
      .accessibilityHidden(true)
  }

  /// The pane's category symbol.
  var symbol: SymbolName {
    switch pane {
    case .helper: .display
    case .general: .gear
    case .agents: .terminal
    case .screen: .screens
    case .notifications: .notifications
    case .about: .info
    }
  }

  /// The category's stable fill.
  private var fill: Color {
    switch pane {
    case .helper: .accent
    case .agents: .success
    case .screen: .accentFill
    case .notifications: .attention
    case .general, .about: .fillControlRaised
    }
  }

  /// The glyph's contrasting color.
  private var foreground: Color {
    switch pane {
    case .helper: .onAccent
    case .agents: .onSuccess
    case .screen: .textLink
    case .notifications: .onAttention
    case .general, .about: .textPrimary
    }
  }
}
