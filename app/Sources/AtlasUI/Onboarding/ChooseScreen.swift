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
      Headline(title: "Agents need an account", tail: "of their own.")
      Lede(
        text:
          "A standard account on this Mac that only Atlas uses. Your own files and windows stay out of its reach."
      )
      VStack(spacing: Spacing.m) {
        ChoiceRow(
          title: "Create a new account",
          detail: "Recommended. A standard account named Atlas, with Atlas's picture.",
          mark: .symbol("plus"),
          selected: state.isNewAccountPicked
        ) { send(.pick(.newAccount)) }
        ForEach(state.accounts) { account in
          ChoiceRow(
            title: account.title,
            detail: account.detail,
            mark: account.mark,
            selected: account.isSelected
          ) { send(.pick(account.id)) }
        }
      }
      .padding(.top, Spacing.s)
      WorkNoteLine(note: note)
      Note(text: "Administrator accounts are not listed: a helper must be a standard account.")
    }
  }
}

#if DEBUG
  #Preview("Choose") {
    ChooseScreen(state: PreviewStates.onboardingChooseScreen, note: .none) { _ in }
      .onboardingSample()
  }
#endif
