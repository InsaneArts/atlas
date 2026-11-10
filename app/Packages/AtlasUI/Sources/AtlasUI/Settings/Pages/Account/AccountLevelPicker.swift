import SwiftUI

/// An account's level as three tiles side by side, Look Only, Ask Before Sending and Trusted, the chosen one ringed. A
/// click picks a tile; with the keyboard's focus, the left and right arrows step through them. While a change runs it
/// stays focusable and simply takes no new pick, so the focus is never lost and the arrows work again once it ends. VoiceOver reads it as
/// the system's own picker, as ``Tabs`` does: "What agents can do here", its value the level chosen, each level a
/// choice, adjustable up and down.
struct AccountLevelPicker: View {
  /// The element's name for VoiceOver.
  static let label = "What agents can do here"

  /// The account's name.
  let account: String
  /// The level now.
  let level: AccountLevelChoice
  /// Whether a change runs now: the picker keeps the keyboard's focus and its look, and takes no new pick until it
  /// ends, so the arrows go on working once it does.
  var isBusy = false
  /// Receives the level the person picks.
  let pick: @MainActor (AccountLevelChoice) -> Void
  @FocusState private var focused: Bool
  @Environment(\.isEnabled) private var isEnabled

  /// The tiles.
  var body: some View {
    HStack(spacing: Spacing.m) {
      ForEach(AccountLevelChoice.allCases, id: \.self) { choice in
        Button {
          choose(choice)
        } label: {
          LevelTile(symbol: choice.symbol, title: choice.title, isSelected: choice == level)
        }
        .buttonStyle(.plain)
        .focusable(false)
      }
    }
    .focusable(isEnabled)
    // The chosen tile's ring says where the arrows act; the system's ring round all three would frame the card.
    .focusEffectDisabled()
    .focused($focused)
    .onKeyPress(.leftArrow) { step(-1) }
    .onKeyPress(.rightArrow) { step(1) }
    .accessibilityRepresentation {
      Picker(Self.label, selection: Binding(get: { level }, set: { choose($0) })) {
        ForEach(AccountLevelChoice.allCases, id: \.self) { choice in
          Text(choice.title)
            .tag(choice)
        }
      }
      .pickerStyle(.segmented)
      // Only the name is read; a label drawn beside the choices would push them off the tiles.
      .labelsHidden()
      .accessibilityLabel(Text(Self.label))
      .accessibilityValue(Text(Self.spokenValue(level)))
      .accessibilityHint(Text(verbatim: "In \(account)"))
      .accessibilityAdjustableAction { direction in
        switch direction {
        case .increment: _ = step(1)
        case .decrement: _ = step(-1)
        @unknown default: break
        }
      }
    }
  }

  /// What VoiceOver says the picker holds: the level chosen.
  ///
  /// - Parameter level: The level now.
  /// - Returns: Its title.
  nonisolated static func spokenValue(_ level: AccountLevelChoice) -> String {
    level.title
  }

  /// The level one step from `level`, stopping at either end.
  ///
  /// - Parameters:
  ///   - level: The level now.
  ///   - offset: -1 for the one before, 1 for the one after.
  /// - Returns: The level.
  nonisolated static func stepped(_ level: AccountLevelChoice, by offset: Int) -> AccountLevelChoice {
    let choices = AccountLevelChoice.allCases
    let index = choices.firstIndex(of: level) ?? 0
    return choices[min(max(index + offset, 0), choices.count - 1)]
  }

  /// Picks a level, unless it is the one already chosen.
  ///
  /// - Parameter choice: The level.
  private func choose(_ choice: AccountLevelChoice) {
    if Self.takes(choice, level: level, isBusy: isBusy) { pick(choice) }
  }

  /// Whether a pick goes through: one of another level, while no change runs.
  ///
  /// - Parameters:
  ///   - choice: The level picked.
  ///   - level: The level now.
  ///   - isBusy: Whether a change runs now.
  /// - Returns: Whether it is sent.
  nonisolated static func takes(_ choice: AccountLevelChoice, level: AccountLevelChoice, isBusy: Bool) -> Bool {
    !isBusy && choice != level
  }

  /// Steps to the level beside the chosen one, from an arrow key or VoiceOver's adjustment.
  ///
  /// - Parameter offset: -1 for the one before, 1 for the one after.
  /// - Returns: Handled, so the key goes no further.
  private func step(_ offset: Int) -> KeyPress.Result {
    guard isEnabled else { return .ignored }
    choose(Self.stepped(level, by: offset))
    return .handled
  }
}
