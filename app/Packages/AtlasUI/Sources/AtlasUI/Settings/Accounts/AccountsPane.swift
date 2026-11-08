import SwiftUI

/// The Accounts pane: the note Atlas shows once after it added an account, then the accounts it lists for the helper,
/// each with its level and Remove…, and Add an App… under them. With none, it says so and offers Add an App…; when
/// Atlas can't read its list, it says that instead. The note grows in and leaves in place, and the list below moves
/// with it.
struct AccountsPane: View {
  /// What the pane shows.
  let state: SettingsAccountsState
  /// Receives what the person does.
  let send: @MainActor (SettingsEvent) -> Void
  @Environment(\.motionReduced) private var reduceMotion

  /// What it draws, from its state.
  var body: some View {
    VStack(alignment: .leading, spacing: SettingsLayout.sectionGap) {
      if let note = state.note {
        AccountNoteCard(
          text: note,
          level: notedLevel,
          pick: { level in
            if let id = state.noteAccount { send(.setAccountLevel(id, level)) }
            send(.dismissAccountNote)
          },
          dismiss: { send(.dismissAccountNote) }
        )
        .transition(Motion.transition(AccountsPane.arrival, reduceMotion: reduceMotion))
      }
      content
    }
    .animation(Motion.animation(Motion.state, reduceMotion: reduceMotion), value: state.note)
    .animation(Motion.animation(Motion.state, reduceMotion: reduceMotion), value: state.rows?.map(\.id))
  }

  /// The list, or why there is none.
  @ViewBuilder private var content: some View {
    switch state.rows {
    case .none:
      VStack(alignment: .leading, spacing: Spacing.sPlus) {
        EmptyMessage(
          state: EmptyMessageState(
            headline: "Atlas can't read its accounts list",
            message: "Nothing was changed. Try again in a moment."
          ),
          ground: .pane
        )
        noteLines
      }
    case .some(let rows) where rows.isEmpty:
      VStack(alignment: .leading, spacing: Spacing.sPlus) {
        EmptyMessage(state: state.empty, ground: .pane) { send(.addApp) }
          .disabled(state.isWorking)
        noteLines
      }
    case .some(let rows):
      VStack(alignment: .trailing, spacing: Spacing.m) {
        // The window's title already says Accounts, so the list needs no heading of its own.
        GroupedSection(rows, notes: notes) { row in
          AccountRow(
            row: row,
            isEnabled: !state.isWorking,
            pick: { send(.setAccountLevel(row.id, $0)) },
            remove: { send(.askRemoveAccount(row.id)) }
          )
        }
        Button("Add an App…") { send(.addApp) }
          .buttonStyle(.capsule(.secondary, size: .mini))
          .disabled(state.isWorking)
          .padding(.trailing, Spacing.m)
      }
    }
  }

  /// The level of the account the note is about, which its Change… opens on; nil when the note names none listed.
  private var notedLevel: AccountLevelChoice? {
    state.rows?.first { $0.id == state.noteAccount }?.level
  }

  /// What the last change said, under the empty pane's message, inset as a section's notes are.
  private var noteLines: some View {
    ForEach(notes.indices, id: \.self) { index in
      GroupedSectionNoteView(note: notes[index])
        .padding(.horizontal, Spacing.m)
    }
  }

  /// What the last change said, under the list or the message.
  private var notes: [GroupedSectionNote] {
    [state.notice?.sectionNote]
      .compactMap { $0 }
  }

  /// How the note comes and goes: it grows in from a little smaller as it fades in, in place, and leaves the same way.
  static let arrival = AnyTransition.scale(scale: AlertCardScale.leaving)
    .combined(with: .opacity)
}

#if DEBUG
  #Preview("Accounts") {
    FormColumn(insets: SettingsLayout.swapInsets) {
      AccountsPane(state: PreviewStates.settingsAccounts.accounts) { _ in }
    }
    .frame(width: SettingsLayout.width)
    .sampleSurface()
  }

  #Preview("Accounts, look only") {
    FormColumn(insets: SettingsLayout.swapInsets) {
      AccountsPane(state: PreviewStates.settingsAccountsLookOnly.accounts) { _ in }
    }
    .frame(width: SettingsLayout.width)
    .sampleSurface()
  }

  #Preview("Accounts, none") {
    FormColumn(insets: SettingsLayout.swapInsets) {
      AccountsPane(state: PreviewStates.settingsAccountsEmpty.accounts) { _ in }
    }
    .frame(width: SettingsLayout.width)
    .sampleSurface()
  }
#endif
