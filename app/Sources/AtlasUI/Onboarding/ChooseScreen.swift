import SwiftUI

/// Choose an account to be the helper: a new one, or a standard account already on this Mac.
struct ChooseScreen: View {
  /// The rows to choose from.
  let state: OnboardingChooseScreenState
  /// The line under the rows.
  let note: WorkNote
  /// Receives the account picked.
  let send: @MainActor (OnboardingEvent) -> Void

  /// What a helper is, the rows, and the work note.
  var body: some View {
    ScreenColumn(width: OnboardingLayout.wideColumn) {
      Eyebrow(text: "Helper account")
      Headline(title: OnboardingScreen.chooseTitle, tail: OnboardingScreen.chooseTail)
      Lede(
        text:
          "A standard account on this Mac that only Atlas uses. Your own files and windows stay out of its reach."
      )
      list
        .padding(.top, Spacing.s)
    } bottom: {
      WorkNoteLine(note: note)
      footnote
    }
  }

  /// Who is listed, or why one account is: left out while a problem shows, so the problem has the room.
  @ViewBuilder private var footnote: some View {
    switch (note, state.offersNewAccount) {
    case (.problem, _): EmptyView()
    case (_, true): Note(text: "Administrator accounts are not listed: a helper must be a standard account.")
    case (_, false): Note(text: "Atlas was set up in this account before, so it keeps using it.")
    }
  }

  /// The rows, scrolling in a fixed height once there are more than the window holds, so the footer stays in view.
  @ViewBuilder private var list: some View {
    if rowCount > OnboardingLayout.accountRowsInView {
      ScrollView(.vertical) {
        rows
      }
      .frame(height: OnboardingLayout.accountListHeight)
    } else {
      rows
    }
  }

  /// How many rows the choice has.
  private var rowCount: Int {
    state.accounts.count + (state.offersNewAccount ? 1 : 0)
  }

  /// A new account, when offered, then each standard account.
  private var rows: some View {
    VStack(spacing: Spacing.m) {
      if state.offersNewAccount {
        ChoiceRow(
          title: "Create a new account",
          detail: "Recommended. A standard account named Atlas, with Atlas's picture.",
          mark: .symbol("plus"),
          selected: state.isNewAccountPicked
        ) { send(.pick(.newAccount)) }
      }
      ForEach(state.accounts) { account in
        ChoiceRow(
          title: account.title,
          detail: account.detail,
          mark: account.mark,
          selected: account.isSelected
        ) { send(.pick(account.id)) }
      }
    }
  }
}

#if DEBUG
  #Preview("Choose") {
    ChooseScreen(state: PreviewStates.onboardingChooseScreen, note: .none) { _ in }
      .onboardingSample()
  }
#endif
