import SwiftUI

/// Keeps the last results visible only for their fade out, with input retired at once.
struct SettingsSearchPresentation: View {
  /// The current local search state.
  let state: SettingsSearchState
  /// Receives actions from the active results alone.
  let send: @MainActor (SettingsEvent) -> Void
  /// Whether results are the current or single outgoing surface.
  var retained = true
  /// The one outgoing results snapshot; typing always draws the current state immediately.
  @State private var lastResults: SettingsSearchState?

  /// A stable opacity layer, without a queued removal or an animated list layout.
  var body: some View {
    SettingsSearchResults(state: state.isActive ? state : lastResults ?? state) { event in
      if state.isActive { send(event) }
    }
    .modifier(SettingsVisibility(active: state.isActive, retained: retained))
    .onChange(of: state, initial: true) { _, search in
      if search.isActive { lastResults = search }
    }
  }
}
