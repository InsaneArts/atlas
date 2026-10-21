import SwiftUI

/// The card that teaches project folders: what teaching does, Choose Folder…, and the folders chosen.
struct ProjectsCard: View {
  /// The chosen project folders, in the order chosen.
  let projects: [OnboardingProjectItem]
  /// Whether a command runs, so choosing and removing wait.
  let isBusy: Bool
  /// Receives the folder choice and removals.
  let send: @MainActor (OnboardingEvent) -> Void

  /// What teaching does beside Choose Folder…, then a chip per folder chosen.
  var body: some View {
    CardGroup {
      VStack(alignment: .leading, spacing: Spacing.ml) {
        HStack(spacing: Spacing.l) {
          TitleDetail(
            title: "Teach a project too",
            detail: "Adds a short section to its AGENTS.md, so agents there check their work on screen.",
            wraps: true
          )
          Spacer(minLength: Spacing.m)
          Button("Choose Folder…") { send(.chooseProject) }
            .buttonStyle(.capsule(.glass))
            .disabled(isBusy)
        }
        if !projects.isEmpty {
          WrappingStack(spacing: Spacing.s) {
            ForEach(projects) { project in
              ProjectChip(project: project) { send(.removeProject(id: project.id)) }
            }
          }
          .disabled(isBusy)
        }
      }
      .padding(.horizontal, Spacing.xl)
      .padding(.vertical, Spacing.l)
    }
  }
}

#if DEBUG
  #Preview("One chosen") {
    ProjectsCard(projects: PreviewStates.onboardingAgentsScreen.projects, isBusy: false) { _ in }
      .frame(width: OnboardingLayout.listColumn)
      .sampleSurface()
  }
#endif
