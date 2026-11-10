import SwiftUI

/// One account in the Accounts pane: its first letter on a disc, its name over when it was added, how freely agents
/// act there, as three segments or, where they don't fit, a popup, and a chevron. Clicking the row opens it in place:
/// what else can be done with it, a row each inside the same card, separators inset both sides, growing in on the
/// fold's spring as the chevron turns (``ActivityMotion/disclosure(reduceMotion:)``).
struct AccountRow: View {
  /// The account.
  let row: SettingsAccountRow
  /// Whether the row is open.
  let isOpen: Bool
  /// Whether its controls take presses now.
  let isEnabled: Bool
  /// Receives what the person does.
  let send: @MainActor (SettingsEvent) -> Void
  @Environment(\.motionReduced) private var reduceMotion

  /// The account's row, and what it offers while open.
  var body: some View {
    VStack(spacing: .zero) {
      ListRowBody(row.title, detail: row.detail, lead: .letter(row.letter), size: .compact) {
        HStack(spacing: Spacing.s) {
          ViewThatFits(in: .horizontal) {
            AccountLevelControl(account: row.title, level: row.level, pick: pick)
            AccountLevelMenu(account: row.title, level: row.level, isEnabled: isEnabled, pick: pick) {
              Button("Remove from List…", role: .destructive) { send(.askRemoveAccount(row.id)) }
            }
          }
          chevron
        }
        .disabled(!isEnabled)
        // Offered the row's room before the title, which cuts itself short, so the segments show wherever they fit.
        .layoutPriority(1)
      }
      .contentShape(Rectangle())
      .onTapGesture { send(.toggleAccount(row.id)) }
      .accessibilityAction(named: isOpen ? "Close" : "Open") { send(.toggleAccount(row.id)) }
      if isOpen {
        VStack(spacing: .zero) {
          ForEach(AccountAction.of(row)) { action in
            SeparatorLine()
              .padding(.horizontal, ListRow.Size.compact.inset)
            ListRowBody(action.title, detail: action.detail, size: .compact) {
              Button(action.button) { send(action.event) }
                .buttonStyle(.capsule(.secondary, size: .row))
                .disabled(!isEnabled)
            }
          }
        }
        .transition(ActivityMotion.disclosure(reduceMotion: reduceMotion))
      }
    }
    .animation(ActivityMotion.fold(reduceMotion: reduceMotion), value: isOpen)
  }

  /// The chevron, turning down as the row opens.
  private var chevron: some View {
    Button {
      send(.toggleAccount(row.id))
    } label: {
      Image(systemName: SymbolName.next.rawValue)
        .font(Typography.font(.callout, weight: .semibold))
        .foregroundStyle(Color.textTertiary)
        .rotationEffect(.degrees(isOpen ? AccountsLayout.chevronTurn : .zero))
        .animation(Motion.animation(Motion.state, reduceMotion: reduceMotion), value: isOpen)
        .frame(width: ControlHeight.xs, height: ControlHeight.xs)
        .contentShape(Rectangle())
    }
    .buttonStyle(.plain)
    .help(isOpen ? "Close \(row.title)" : "More for \(row.title)")
    .accessibilityLabel(isOpen ? "Close \(row.title)" : "More for \(row.title)")
  }

  /// Sets the level the person picked.
  ///
  /// - Parameter level: The level.
  private func pick(_ level: AccountLevelChoice) {
    send(.setAccountLevel(row.id, level))
  }
}
