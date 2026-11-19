import SwiftUI

/// A VM's page, opened from Desktops, or in Helper's place while it is the one desktop: its card, then its rows in
/// their sections, each under its heading, its power and maintenance first and Remove… last; while one of its commands
/// runs its buttons wait under a line with a spinner, and what the last one said shows under the last section. Add a
/// Desktop… stands at its foot while it is the one desktop.
struct VMPage: View {
  /// What the page shows.
  let state: SettingsVMState
  /// Receives what the person does.
  let send: @MainActor (SettingsEvent) -> Void

  /// What it draws, from its state.
  var body: some View {
    VStack(alignment: .trailing, spacing: Spacing.sPlus) {
      VStack(alignment: .leading, spacing: SettingsLayout.sectionGap) {
        VMStatusCard(state: state)
        ForEach(Array(state.sections.enumerated()), id: \.element.id) { index, section in
          GroupedSection(
            section.rows,
            title: section.title ?? "",
            notes: index == state.sections.count - 1 ? notes : []
          ) { row in
            VMRowView(row: row, send: send)
          }
        }
      }
      if state.addsDesktop {
        AddDesktopButton(send: send)
      }
    }
  }

  /// The lines under the last section: what runs now, then what the last command said.
  private var notes: [GroupedSectionNote] {
    [state.waiting.map { GroupedSectionNote.status(.working, $0) }, state.notice?.sectionNote]
      .compactMap { $0 }
  }
}

#if DEBUG
  #Preview("A VM, running") {
    SettingsPageSample {
      if let vm = PreviewStates.settingsVM.vm { VMPage(state: vm) { _ in } }
    }
  }

  #Preview("A VM in maintenance, dark") {
    SettingsPageSample {
      if let vm = PreviewStates.settingsVMMaintenance.vm { VMPage(state: vm) { _ in } }
    }
    .environment(\.colorScheme, .dark)
  }
#endif
