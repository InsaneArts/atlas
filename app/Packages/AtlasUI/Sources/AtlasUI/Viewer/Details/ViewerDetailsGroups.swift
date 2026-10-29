import SwiftUI

/// Details' groups of facts: the chosen host session's first, when one is; then the helper; Ready to act, one row
/// while every check passes, else each check that does not with what it means and a count of those that do; the
/// picture; and the helper's login. A group with nothing known yet is left out.
struct ViewerDetailsGroups: View {
  /// What the inspector shows.
  let state: ViewerDetailsState
  /// Reads how old the newest picture is as its own row draws.
  let newest: @MainActor () -> String
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  /// The groups, 14 points apart.
  var body: some View {
    VStack(alignment: .leading, spacing: ViewerLayout.detailsInset) {
      if let sessionDetail = state.sessionDetail {
        SessionDetailGroups(detail: sessionDetail, send: send)
      }
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
      if !state.login.isEmpty {
        FactGroup("Login") {
          ForEach(state.login) { FactRow($0.name, value: $0.value) }
        }
      }
    }
    .frame(maxWidth: .infinity, alignment: .leading)
  }
}

#if DEBUG
  #Preview("Some checks not passing") {
    if let details = PreviewStates.viewerDetailsChecks.details {
      ViewerDetailsGroups(state: details, newest: { details.newest }) { _ in }
        .frame(width: ViewerLayout.detailsWidth)
        .sampleSurface()
    }
  }

  #Preview("A session chosen") {
    if let details = PreviewStates.viewerSessionDetail.details {
      ViewerDetailsGroups(state: details, newest: { details.newest }) { _ in }
        .frame(width: ViewerLayout.detailsWidth)
        .sampleSurface()
    }
  }
#endif
