import SwiftUI

/// Choose an account to be the helper: a new one, or a standard account already on this Mac, in one group whose radio
/// is the only mark of the choice. With one account alone there is nothing to choose, so its row has no radio. Past
/// three and a half rows the group scrolls inside itself, so the footer never moves. A problem setting an account up
/// shows in that account's row. Continue is the footer's.
struct ChooseScreen: View {
  /// The rows to choose from.
  let state: OnboardingChooseScreenState
  /// The status under the rows.
  let note: WorkNote
  /// Receives the account picked.
  let send: @MainActor (OnboardingEvent) -> Void

  /// What a helper is, the rows, the status and the footnote, which keeps its three lines whatever it says, so picking
  /// an account moves nothing.
  var body: some View {
    ScreenColumn {
      OnboardingHeader(
        label: "Helper account",
        title: OnboardingScreen.chooseTitle,
        tail: OnboardingScreen.chooseTail,
        lede:
          "A standard account on this Mac that only Atlas uses. Your own files and windows stay out of its reach."
      )
      GroupedList(Self.rows(state), scrolling: .regular) { row in
        ListRow(
          row.title,
          detail: row.detail,
          detailStyle: row.detailStyle,
          tag: row.tag,
          titleFit: row.titleFit,
          lead: row.lead,
          trailing: row.trailing
        ) { send(.pick(row.id)) }
      }
      .padding(.top, OnboardingLayout.Rhythm.paragraph)
      StatusSlot(note: note)
      Note(text: footnote)
        .lineLimit(OnboardingLayout.footnoteLines, reservesSpace: true)
        .padding(.top, OnboardingLayout.Rhythm.status)
    }
  }

  /// What approving will do for the account picked, else who is listed, or why one account is.
  private var footnote: String {
    switch (state.approval, state.offersNewAccount) {
    case (.some(let approval), _): approval
    case (nil, true): "Administrator accounts are not listed: the helper account must be a standard account."
    case (nil, false): "Atlas was set up in this account before, so it keeps using it."
    }
  }

  /// A new account, when offered, with the helper's own picture and its Recommended tag, then each standard account
  /// with its initial, its name on one line. Each ends in its radio, unless it is the one row, which needs none; an
  /// account a problem is about shows the problem in place of its detail.
  ///
  /// - Parameter state: The choice.
  /// - Returns: The rows, in order.
  static func rows(_ state: OnboardingChooseScreenState) -> [ListRowItem<OnboardingAccountPick>] {
    let newAccount = ListRowItem(
      id: OnboardingAccountPick.newAccount,
      title: "Create a new account",
      detail: "A standard account named Atlas, with Atlas’s picture.",
      tag: "Recommended",
      lead: .picture,
      trailing: .radio(isOn: state.isNewAccountPicked)
    )
    let accounts = state.accounts.map { account in
      ListRowItem(
        id: account.id,
        title: account.title,
        detail: account.problem ?? account.detail,
        detailStyle: account.problem == nil ? .plain : .problem,
        titleFit: .oneLine,
        lead: lead(account.mark),
        trailing: .radio(isOn: account.isSelected)
      )
    }
    let rows = (state.offersNewAccount ? [newAccount] : []) + accounts
    guard rows.count == 1 else { return rows }
    // One account alone is no choice: no radio.
    return rows.map { row in
      var row = row
      row.trailing = .none
      return row
    }
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
