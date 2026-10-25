#if DEBUG
  import Foundation

  extension PreviewStates {
    /// The phase's search presentation cases, including the unchanged page for an empty query.
    @MainActor static var settingsSearch: [PreviewState<SettingsState>] {
      [
        PreviewState(name: "Search, empty query", value: searchSample("", targets: [])),
        PreviewState(
          name: "Search, ranked results",
          value: searchSample("helper", targets: [.page(.root(.helper)), .recover, .logOut, .account, .readiness])
        ),
        PreviewState(name: "Search, one result", value: searchSample("startup", targets: [.startup])),
        PreviewState(name: "Search, no results", value: searchSample("violet telescope", targets: [])),
        PreviewState(
          name: "Search, long query",
          value: searchSample(String(repeating: "a very long query ", count: 24), targets: [])
        ),
        PreviewState(name: "Search, long breadcrumbs", value: settingsSearchLongBreadcrumbs),
        PreviewState(name: "Search, active alert", value: settingsSearchAlert),
      ]
    }

    /// Curated matches make previews independent of the app's search implementation.
    ///
    /// - Parameters:
    ///   - query: The field text.
    ///   - targets: The fixture's ordered results.
    /// - Returns: A static search presentation.
    @MainActor static func searchSample(_ query: String, targets: [SettingsSearchTarget]) -> SettingsState {
      var state = settingsGeneral
      state.search.query = query
      let entries = SettingsSearchCatalog.entries(state)
      state.search.results = targets.compactMap { target in entries.first { $0.id == target } }
      state.search.selection = state.search.results.first?.id
      return state
    }

    /// The interactive sample's local matches; production ranking lives in the app's pure index.
    ///
    /// - Parameters:
    ///   - query: The sample field text.
    ///   - state: The sample's already loaded metadata.
    /// - Returns: The matching sample rows in presentation order.
    @MainActor static func settingsSearchMatches(_ query: String, in state: SettingsState) -> [SettingsSearchEntry] {
      let terms = query.split(whereSeparator: \.isWhitespace)
      guard !terms.isEmpty else { return [] }
      return SettingsSearchCatalog.entries(state)
        .filter { entry in
          let text = ([entry.title, entry.detail, entry.section, entry.sectionDetail] + entry.aliases)
            .joined(
              separator: " "
            )
          return terms.allSatisfy { text.localizedStandardContains(String($0)) }
        }
    }

    /// Wrapping in both the breadcrumb and project name remains readable.
    @MainActor private static var settingsSearchLongBreadcrumbs: SettingsState {
      var state = settingsGeneral
      state.search.query = "project"
      state.search.results = [
        SettingsSearchEntry(
          id: .project("/sample/projects/research"),
          title: "Research and development workspace for the desktop application",
          detail: "~/Projects/Research and development/workspaces/desktop-application",
          section: "Projects with long names and several nested workspace folders"
        )
      ]
      state.search.selection = state.search.results.first?.id
      return state
    }

    /// The single alert host stays above results and disables the underlying shell.
    @MainActor private static var settingsSearchAlert: SettingsState {
      var state = searchSample("helper", targets: [.recover, .logOut])
      state.alert = settingsRecover.alert
      return state
    }
  }
#endif
