import SwiftUI

/// The existing taught projects and Forget action, kept in one scrolling page.
struct ProjectsPane: View {
  /// The owning Agents state, including command notices.
  let state: SettingsAgentsState
  /// Receives each path-based Forget request.
  let send: @MainActor (SettingsEvent) -> Void

  /// Empty and populated lists both preserve the explanation and any command failure.
  var body: some View {
    VStack(alignment: .leading, spacing: SettingsLayout.titleGap) {
      SettingsSectionHeading(title: "Projects")
      if state.projects.isEmpty {
        GroupedSection([GeneralRow.picture], notes: notes) { _ in
          ListRow("No projects taught", size: .compact)
        }
      } else {
        GroupedSection(state.projects, notes: notes) { project in
          ProjectRow(project: project) { send(.askForget(project.id)) }
        }
      }
    }
  }

  /// The existing explanation and the owning Agents command's result.
  private var notes: [GroupedSectionNote] {
    [
      .text("Agents without the Atlas plugin read these projects' AGENTS.md. Forget takes Atlas's section out."),
      state.notice?.sectionNote,
    ].compactMap { $0 }
  }
}
