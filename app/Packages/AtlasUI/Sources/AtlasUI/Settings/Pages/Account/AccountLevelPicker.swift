import SwiftUI

/// An account's level as three tiles side by side, Look Only, Ask Before Sending and Trusted, the chosen one ringed. A
/// click picks a tile; with the keyboard's focus, the left and right arrows step through them. VoiceOver reads it as one
/// labelled picker, adjustable up and down.
struct AccountLevelPicker: View {
  /// The account's name, which VoiceOver reads with the picker.
  let account: String
  /// The level now; nil while unknown, which rings none.
  let level: AccountLevelChoice?
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
    .focused($focused)
    .onKeyPress(.leftArrow) { step(-1) }
    .onKeyPress(.rightArrow) { step(1) }
    .accessibilityRepresentation {
      Picker("What agents may do in \(account)", selection: Binding(get: { level }, set: { $0.map(choose) })) {
        ForEach(AccountLevelChoice.allCases, id: \.self) { choice in
          Text(choice.title)
            .tag(Optional(choice))
        }
      }
      .pickerStyle(.segmented)
    }
  }

  /// Picks a level, unless it is the one already chosen.
  ///
  /// - Parameter choice: The level.
  private func choose(_ choice: AccountLevelChoice) {
    if choice != level { pick(choice) }
  }

  /// Steps to the level beside the chosen one, stopping at either end.
  ///
  /// - Parameter offset: -1 for the one before, 1 for the one after.
  /// - Returns: Handled, so the key goes no further.
  private func step(_ offset: Int) -> KeyPress.Result {
    let choices = AccountLevelChoice.allCases
    let index = level.flatMap { choices.firstIndex(of: $0) } ?? 0
    let next = min(max(index + offset, 0), choices.count - 1)
    choose(choices[next])
    return .handled
  }
}
