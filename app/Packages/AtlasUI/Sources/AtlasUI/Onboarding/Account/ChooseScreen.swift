import SwiftUI

/// Choose an account to be the helper: a new one, or a standard account already on this Mac, in one group whose radio
/// is the only mark of the choice (v3 D6, D7). Past three and a half rows the group scrolls inside itself, so the
/// footer never moves (v3 D10). Continue is the footer's.
struct ChooseScreen: View {
  /// The rows to choose from.
  let state: OnboardingChooseScreenState
  /// The status under the rows.
  let note: WorkNote
  /// Receives the account picked.
  let send: @MainActor (OnboardingEvent) -> Void

  /// What a helper is, the rows, the status and who is listed.
  var body: some View {
    ScrollColumn(width: OnboardingLayout.column) {
      OnboardingHeader(
        label: "Helper account",
        title: OnboardingScreen.chooseTitle,
        tail: OnboardingScreen.chooseTail,
        lede:
          "A standard account on this Mac that only Atlas uses. Your own files and windows stay out of its reach."
      )
      GroupedList(Self.rows(state), scrolling: .regular) { row in
        ListRow(row.title, detail: row.detail, lead: row.lead, trailing: row.trailing) { send(.pick(row.id)) }
      }
      .padding(.top, OnboardingLayout.Rhythm.paragraph)
      .pagePiece(.content)
    } bottom: {
      if let status = StatusLine(note: note) {
        status
          .padding(.top, OnboardingLayout.Rhythm.content)
          .pagePiece(.status)
      }
      if let footnote {
        Note(text: footnote)
          .padding(.top, OnboardingLayout.Rhythm.status)
          .pagePiece(.status)
      }
    }
  }

  /// What approving will do for the account picked, else who is listed, or why one account is: left out while a
  /// problem shows, so the problem has the room.
  private var footnote: String? {
    switch (note, state.approval, state.offersNewAccount) {
    case (.problem, _, _): nil
    case (_, .some(let approval), _): approval
    case (_, nil, true): "Administrator accounts are not listed: a helper must be a standard account."
    case (_, nil, false): "Atlas was set up in this account before, so it keeps using it."
    }
  }

  /// A new account, when offered, with the helper's own picture, then each standard account with its initial, each
  /// ending in its radio.
  ///
  /// - Parameter state: The choice.
  /// - Returns: The rows, in order.
  static func rows(_ state: OnboardingChooseScreenState) -> [ListRowItem<OnboardingAccountPick>] {
    let newAccount = ListRowItem(
      id: OnboardingAccountPick.newAccount,
      title: "Create a new account",
      detail: "Recommended. A standard account named Atlas, with Atlas's picture.",
      lead: .picture,
      trailing: .radio(isOn: state.isNewAccountPicked)
    )
    let accounts = state.accounts.map { account in
      ListRowItem(
        id: account.id,
        title: account.title,
        detail: account.detail,
        lead: lead(account.mark),
        trailing: .radio(isOn: account.isSelected)
      )
    }
    return (state.offersNewAccount ? [newAccount] : []) + accounts
  }

  /// What leads an account's row: its initial on an avatar disc, or a symbol on a tile.
  ///
  /// - Parameter mark: What the account's disc shows.
  /// - Returns: The row's lead.
  private static func lead(_ mark: AccountMark) -> ListRow.Lead {
    switch mark {
    case .letter(let letter): .letter(letter)
    case .symbol(let symbol): .tile(.symbol(symbol))
    }
  }
}

#if DEBUG
  #Preview("Choose") {
    ChooseScreen(state: PreviewStates.onboardingChooseScreen, note: .none) { _ in }
      .onboardingSample()
  }

  #Preview("Many accounts") {
    if case .screen(.choose(let state)) = PreviewStates.onboardingChooseMany.content {
      ChooseScreen(state: state, note: .none) { _ in }
        .onboardingSample()
    }
  }
#endif
