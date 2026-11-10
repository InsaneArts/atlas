import SwiftUI

/// An account's own page, pushed over Accounts: its card with its letter, name and what it is; "What Agents Can Do
/// Here", its three level tiles over what the chosen one lets agents do; for a site, "Sign-In" with Sign Out…, Forget…
/// and Reset…, each opening the helper's browser with agents paused; then Remove…, which asks first. While Atlas can't
/// read its list the page keeps its name, says so in place of its level, and its choices wait; Back still works.
struct AccountPage: View {
  /// What the page shows.
  let state: SettingsAccountPageState
  /// Receives what the person does.
  let send: @MainActor (SettingsEvent) -> Void

  /// What it draws, from its state.
  var body: some View {
    VStack(alignment: .leading, spacing: SettingsLayout.sectionGap) {
      AccountHeaderCard(letter: state.letter, title: state.title, line: state.kindLine)
      if state.isUnreadable {
        AccountsEmptyCard(state: AccountsPage.unreadable)
      } else {
        levelSection
        if state.isSite {
          GroupedSection(AccountAction.signIn(of: state.id), title: "Sign-In") { action in
            actionRow(action, look: .secondary)
          }
        }
        GroupedSection([AccountAction.remove(state.id)], notes: notes) { action in
          actionRow(action, look: .destructive)
        }
      }
    }
    .disabled(!state.isEnabled)
  }

  /// The level's tiles over what it lets agents do.
  private var levelSection: some View {
    VStack(alignment: .leading, spacing: SettingsLayout.titleGap) {
      SectionTitle(text: "What Agents Can Do Here")
        .padding(.horizontal, ListRow.Size.compact.inset)
      VStack(spacing: .zero) {
        AccountLevelPicker(account: state.title, level: state.level) { send(.setAccountLevel(state.id, $0)) }
          .padding(.horizontal, ListRow.Size.compact.inset)
          .padding(.top, Spacing.m)
          .padding(.bottom, Spacing.sPlus)
        Text(state.levelLine)
          .textStyle(.body)
          .foregroundStyle(Color.textSecondary)
          .swapsText(on: state.levelLine)
          .frame(maxWidth: .infinity, minHeight: ListRow.Size.compact.height, alignment: .leading)
          .padding(.horizontal, ListRow.Size.compact.inset)
      }
      .background(
        RoundedRectangle(cornerRadius: Radius.large, style: .continuous)
          .fill(Color.fillGroupInset)
      )
    }
  }

  /// One action's row: what it does and its button.
  ///
  /// - Parameters:
  ///   - action: The action.
  ///   - look: Its button's look.
  /// - Returns: The row.
  private func actionRow(_ action: AccountAction, look: CapsuleButtonStyle.Look) -> some View {
    ListRowBody(action.title, detail: action.detail, size: .compact) {
      Button(action.button) { send(action.event) }
        .buttonStyle(.capsule(look, size: .row))
    }
  }

  /// What the last change on the page said.
  private var notes: [GroupedSectionNote] {
    [state.notice?.sectionNote]
      .compactMap { $0 }
  }
}

#if DEBUG
  #Preview("A site") {
    SettingsPageSample { AccountPage(state: PreviewStates.settingsAccountSitePage) { _ in } }
  }

  #Preview("An app, dark") {
    SettingsPageSample { AccountPage(state: PreviewStates.settingsAccountAppPage) { _ in } }
      .environment(\.colorScheme, .dark)
  }

  #Preview("The list can't be read") {
    SettingsPageSample { AccountPage(state: PreviewStates.settingsAccountUnreadablePage) { _ in } }
  }
#endif
