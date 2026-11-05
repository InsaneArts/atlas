import SwiftUI

/// The General pane: opening at login and how often the screen updates; then Activity, with pictures of agent actions,
/// how long they are kept, how much typed text is kept, and how many this Mac holds with Delete All…; then when Atlas
/// needs the person. Its values change through the binding; Delete All… asks first.
struct GeneralPane: View {
  /// The pane's values.
  @Binding var state: SettingsGeneralState
  /// Receives what the person does other than change a value.
  let send: @MainActor (SettingsEvent) -> Void

  /// What it draws, from its state.
  var body: some View {
    VStack(alignment: .leading, spacing: SettingsLayout.sectionGap) {
      GroupedSection(GeneralRow.start) { row in
        self.row(row)
      }
      GroupedSection(GeneralRow.activity(state), title: "Activity", notes: activityNotes) { row in
        self.row(row)
      }
      GroupedSection(GeneralRow.needs, title: "When Atlas needs you") { row in
        self.row(row)
      }
    }
  }

  /// The lines under Activity: what the last change said went wrong, else where pictures stay.
  private var activityNotes: [GroupedSectionNote] {
    [state.notice?.sectionNote ?? .text("Pictures stay on this Mac, in Atlas's private folder.")]
  }

  /// One row: its words and its control.
  ///
  /// - Parameter row: The row.
  /// - Returns: The row.
  @ViewBuilder private func row(_ row: GeneralRow) -> some View {
    switch row {
    case .openAtLogin:
      ListRow(
        "Open Atlas at login",
        detail: "Atlas sits in the menu bar from the start",
        trailing: .toggle(isOn: state.opensAtLogin),
        size: .compact
      ) { state.opensAtLogin.toggle() }
      .disabled(!state.canOpenAtLogin)
    case .picture:
      ListRowBody("Screen updates", detail: "How often Atlas's windows show the helper's screen", size: .compact) {
        PopupButton(label: "Screen updates", options: state.pictureOptions, selection: $state.picture)
      }
    case .picturesBefore:
      ListRow(
        "Pictures of agent actions",
        detail: "A picture before and after each click, keystroke and scroll. Agents never see them.",
        trailing: .toggle(isOn: state.picturesBeforeActions),
        size: .compact
      ) { state.picturesBeforeActions.toggle() }
    case .keepFor:
      ListRowBody("Keep pictures for", size: .compact) {
        PopupButton(label: "Keep pictures for", options: Self.keepOptions, selection: $state.keepFor)
      }
    case .keepDays:
      ListRowBody("Days", size: .compact) {
        Stepper(value: $state.keepDays, in: SettingsGeneralState.keepDaysRange) {
          Text(state.keepDays == 1 ? "1 day" : "\(state.keepDays) days")
            .font(Typography.digits(.callout))
        }
        .fixedSize()
      }
    case .typedText:
      ListRowBody("Typed text", detail: "What an agent typed, kept beside its picture", size: .compact) {
        PopupButton(label: "Typed text", options: Self.typedTextOptions, selection: $state.typedText)
      }
    case .typedTextLimit:
      ListRowBody("Characters", size: .compact) {
        Stepper(
          value: $state.typedTextLimit,
          in: SettingsGeneralState.typedTextRange,
          step: SettingsGeneralState.typedTextStep
        ) {
          Text("Up to \(state.typedTextLimit)")
            .font(Typography.digits(.callout))
        }
        .fixedSize()
      }
    case .stored:
      ListRow(
        "Pictures on this Mac",
        detail: state.storedPictures.isEmpty ? nil : state.storedPictures,
        trailing: .button("Delete All…"),
        size: .compact
      ) { send(.askDeletePictures) }
      .disabled(!state.canDeletePictures)
    case .notify:
      ListRow(
        "Notify me",
        detail: "When an action may not have finished, or the helper logs out",
        trailing: .toggle(isOn: state.notify),
        size: .compact
      ) { state.notify.toggle() }
    }
  }

  /// How long pictures are kept: three days, seven, or a number the person chooses.
  private static let keepOptions = [
    PopupOption(id: SettingsKeepFor.threeDays, title: "3 days"),
    PopupOption(id: SettingsKeepFor.sevenDays, title: "7 days"),
    PopupOption(id: SettingsKeepFor.custom, title: "Custom…"),
  ]

  /// How much typed text is kept: all of it, or up to a number of characters.
  private static let typedTextOptions = [
    PopupOption(id: SettingsTypedText.full, title: "Full"),
    PopupOption(id: SettingsTypedText.limited, title: "Limited…"),
  ]
}

#if DEBUG
  #Preview("General") {
    FormColumn(insets: SettingsLayout.swapInsets) {
      GeneralPane(state: .constant(PreviewStates.settingsGeneral.general)) { _ in }
    }
    .frame(width: SettingsLayout.width)
    .sampleSurface()
  }

  #Preview("Custom days and a limit, dark") {
    FormColumn(insets: SettingsLayout.swapInsets) {
      GeneralPane(state: .constant(PreviewStates.settingsGeneralCustom.general)) { _ in }
    }
    .frame(width: SettingsLayout.width)
    .sampleSurface()
    .environment(\.colorScheme, .dark)
  }
#endif
