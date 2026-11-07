import SwiftUI

/// Activity or Apps as the stage keeps it once shown (``KeptPaneShift``), filling the frame inside the
/// window's inset.
struct KeptPane: View {
  /// The pane.
  let pane: ViewerPane
  /// The namespace a screenshot zooms in between its tile and the large view, the window's.
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  /// The pane's view; the Screen is not kept, and draws nothing here.
  var body: some View {
    switch pane {
    case .screen:
      EmptyView()
    case .activity(let activity):
      ViewerActivityPane(state: activity, send: send)
    case .apps(let apps):
      ViewerAppsPane(state: apps, send: send)
    }
  }

}
