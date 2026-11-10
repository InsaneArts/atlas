import SwiftUI

/// The Activity page: whether Atlas keeps pictures of agent actions, how long, and how much typed text beside them,
/// with where they stay; then how many this Mac holds with Delete All…, which asks first. A change that failed says why
/// under the pictures. Its values change through the binding.
struct ActivityPage: View {
  /// The page's values, shared with the General page.
  @Binding var state: SettingsGeneralState
  /// Receives what the person does other than change a value.
  let send: @MainActor (SettingsEvent) -> Void

  /// What it draws, from its state.
  var body: some View {
    VStack(alignment: .leading, spacing: SettingsLayout.sectionGap) {
      GroupedSection(ActivityRow.pictures(state), title: "Pictures of Agent Actions", notes: notes) { row in
        self.row(row)
      }
      GroupedSection([ActivityRow.stored], title: "Storage") { row in
        self.row(row)
      }
    }
  }

  /// The lines under the pictures: what the last change said went wrong, else where pictures stay.
  private var notes: [GroupedSectionNote] {
    [
      state.notice?.sectionNote
        ?? .text("Agents never see these pictures. They stay on this Mac, in Atlas's private folder.")
    ]
  }

  /// One row: its words and its control.
  ///
  /// - Parameter row: The row.
  /// - Returns: The row.
  @ViewBuilder private func row(_ row: ActivityRow) -> some View {
    switch row {
    case .picturesBefore:
      ListRow(
        "Keep pictures",
        detail: "A picture before and after each click, keystroke and scroll",
        trailing: .toggle(isOn: state.picturesBeforeActions),
        size: .compact
      ) { state.picturesBeforeActions.toggle() }
    case .keepFor:
      ListRowBody("Keep pictures for", size: .compact) {
        PopupButton(label: "Keep pictures for", options: Self.keepOptions, selection: $state.keepFor)
      }
    case .keepDays:
      ListRowBody("Days", size: .compact) {
        SettingsNumberField(label: "Days", value: $state.keepDays, range: SettingsGeneralState.keepDaysRange)
      }
    case .typedText:
      ListRowBody("Typed text", detail: "What an agent typed, kept beside its picture", size: .compact) {
        Tabs(state: typedTextTabs, style: .segment) { picked in state.typedText = picked }
          .fixedSize()
      }
    case .typedTextLimit:
      ListRowBody("Characters", size: .compact) {
        SettingsNumberField(
          label: "Characters",
          value: $state.typedTextLimit,
          range: SettingsGeneralState.typedTextRange,
          step: SettingsGeneralState.typedTextStep
        )
      }
    case .stored:
      ListRowBody(
        "Pictures on this Mac",
        detail: state.storedPictures.isEmpty ? nil : state.storedPictures,
        size: .compact
      ) {
        Button("Delete All…") { send(.askDeletePictures) }
          .buttonStyle(.capsule(.destructive, size: .row))
          .disabled(!state.canDeletePictures)
      }
    }
  }

  /// How much typed text is kept, as segments: all of it, or up to a number of characters.
  private var typedTextTabs: CapsuleTabsState<SettingsTypedText> {
    CapsuleTabsState(
      tabs: [
        CapsuleTab(id: SettingsTypedText.full, title: "Full"),
        CapsuleTab(id: SettingsTypedText.limited, title: "Limited"),
      ],
      selected: state.typedText,
      label: "Typed text"
    )
  }

  /// How long pictures are kept: three days, seven, or a number the person chooses.
  private static let keepOptions = [
    PopupOption(id: SettingsKeepFor.threeDays, title: "3 days"),
    PopupOption(id: SettingsKeepFor.sevenDays, title: "7 days"),
    PopupOption(id: SettingsKeepFor.custom, title: "Custom…"),
  ]
}

#if DEBUG
  #Preview("Activity") {
    SettingsPageSample { ActivityPage(state: .constant(PreviewStates.settingsGeneral.general)) { _ in } }
  }

  #Preview("Custom days and a limit, dark") {
    SettingsPageSample { ActivityPage(state: .constant(PreviewStates.settingsGeneralCustom.general)) { _ in } }
      .environment(\.colorScheme, .dark)
  }
#endif
