import AtlasUI

/// Local search changes only session state and visits; no search event can execute a result's control.
extension SettingsController {
  /// Adds search presentation to the current page state, reusing normalized metadata across polls and keystrokes.
  ///
  /// - Parameter state: The current page presentation.
  /// - Returns: The transient search presentation.
  func searchState(for state: SettingsState) -> SettingsSearchState {
    let entries = SettingsSearchCatalog.entries(state)
    if searchIndex?.entries != entries { searchIndex = SettingsSearchIndex(entries: entries) }
    var search = session.search
    search.results = searchIndex?.results(for: search.query) ?? []
    if !search.results.contains(where: { $0.id == search.selection }) { search.selection = search.results.first?.id }
    return search
  }

  /// Edits the local query immediately; it never refreshes the agents or saves a preference.
  ///
  /// - Parameter query: The field's text, including whitespace.
  func search(_ query: String) {
    guard session.alert == nil, session.search.query != query else { return }
    session.reveal = nil
    session.search.query = query
    session.search.selection = nil
    session.search.selection = state.search.selection
  }

  /// Moves within current results, retaining the field's editing focus.
  ///
  /// - Parameter step: A positive or negative arrow-key step.
  func moveSearchSelection(_ step: Int) {
    guard session.alert == nil else { return }
    let search = state.search
    guard !search.results.isEmpty else { return }
    let index = search.results.firstIndex { $0.id == search.selection } ?? 0
    let next = min(max(index + step, 0), search.results.count - 1)
    session.search.selection = search.results[next].id
  }

  /// Opens only a still-current result, clearing the query and requesting inert target focus.
  ///
  /// - Parameter target: The selected result identity, or nil for no match.
  func openSearchResult(_ target: SettingsSearchTarget?) {
    guard session.alert == nil, let target, state.search.results.contains(where: { $0.id == target }) else { return }
    session.search.query = ""
    session.search.selection = nil
    let request = (session.reveal?.request ?? 0) + 1
    session.navigation.visit(target.destination)
    nameWindow?("Settings — \(target.destination.title)")
    session.reveal = SettingsReveal(target: target, request: request)
  }
}
