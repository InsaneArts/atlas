import SwiftUI

/// Details' groups of facts: the helper; Ready to act, one row while every check passes, else each check that
/// does not with what it means and a count of those that do; the picture; and the session. A group with
/// nothing known yet is left out.
struct ViewerDetailsGroups: View {
  /// What the inspector shows.
  let state: ViewerDetailsState
  /// Reads how old the newest picture is as its own row draws.
  let newest: @MainActor () -> String

  /// The groups, 14 points apart.
  var body: some View {
    VStack(alignment: .leading, spacing: ViewerLayout.detailsInset) {
      FactGroup("Helper") {
        ForEach(state.helper) { FactRow($0.name, value: $0.value) }
      }
      if !state.ready.isEmpty {
        FactGroup("Ready to act") {
          ForEach(state.ready) { row in
            FactRow(row.name, value: row.note ?? row.value, mark: row.mark)
          }
        }
      }
      FactGroup("Picture") {
        FactRow("Updates", value: state.updates)
        NewestPictureRow(newest: newest)
      }
      if !state.session.isEmpty {
        FactGroup("Session") {
          ForEach(state.session) { FactRow($0.name, value: $0.value) }
        }
      }
    }
    .frame(maxWidth: .infinity, alignment: .leading)
  }
}

#if DEBUG
  #Preview("Some checks not passing") {
    if let details = PreviewStates.viewerDetailsChecks.details {
      ViewerDetailsGroups(state: details) { details.newest }
        .frame(width: ViewerLayout.detailsWidth)
        .sampleSurface()
    }
  }
#endif
