import AtlasControl
import AtlasService
import AtlasUI
import CoreGraphics

extension ViewerPresenter {
  /// The Apps pane: how many apps and windows are open, the search once any is, and the apps front to back with their
  /// windows, or why none shows. With no app open the header says nothing beside its title, and there is nothing to
  /// search.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The pane.
  static func apps(_ snapshot: ViewerSnapshot) -> ViewerAppsState {
    let facts = snapshot.panes.apps
    guard let apps = facts.apps else {
      let failed = facts.failure.map {
        EmptyMessageState(headline: ViewerWords.appsFailed, message: Words.sentence($0))
      }
      return ViewerAppsState(meta: "", query: facts.query, apps: [], empty: failed, isSearchable: false)
    }
    let rows = apps.compactMap { $0.matching(facts.query) }
      .map { appRow($0, icons: facts.icons) }
    let empty: EmptyMessageState? =
      if !rows.isEmpty {
        nil
      } else if apps.isEmpty {
        EmptyMessageState(headline: ViewerWords.noApps, message: ViewerWords.noAppsMessage)
      } else {
        EmptyMessageState(headline: ViewerWords.noAppsMatch(facts.query), message: ViewerWords.noAppsMatchMessage)
      }
    return ViewerAppsState(
      meta: ViewerWords.appsMeta(apps: apps.count, windows: apps.map(\.windows.count).reduce(0, +)),
      query: facts.query,
      apps: rows,
      empty: empty,
      isSearchable: !apps.isEmpty,
      findRequests: facts.findRequests
    )
  }

  /// An app's row and its windows' rows.
  ///
  /// - Parameters:
  ///   - app: The app, with the windows the search keeps.
  ///   - icons: The icons decoded so far.
  /// - Returns: The row.
  static func appRow(_ app: HelperApp, icons: [String: IconImage]) -> ViewerAppRow {
    ViewerAppRow(
      id: app.pid,
      name: app.name.isEmpty ? ViewerWords.unnamedApp : app.name,
      icon: icons[app.iconID],
      isFront: app.front,
      windows: app.windows.map { window in
        let title = windowTitle(window)
        return ViewerWindowRow(
          id: window.id,
          title: title,
          size: windowSize(window),
          showLabel: ViewerWords.showWindowLabel(title)
        )
      }
    )
  }

  /// The window shown from the Apps tab, outlined over the picture: its edges as shares of the helper's
  /// display, which the view letterboxes as a click maps. None before the picture's display is known.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The outline, or nil.
  static func outline(_ snapshot: ViewerSnapshot) -> ViewerWindowOutline? {
    guard let shown = snapshot.panes.apps.shown, let display = snapshot.reading.frame?.display else { return nil }
    let window = shown.window
    guard let start = DesktopPoint(x: window.x, y: window.y).fraction(of: display),
      let end = DesktopPoint(x: window.x + window.width, y: window.y + window.height).fraction(of: display)
    else { return nil }
    return ViewerWindowOutline(
      frame: CGRect(x: start.x, y: start.y, width: end.x - start.x, height: end.y - start.y),
      title: ViewerWords.outlineTitle(app: shown.app, window: windowTitle(window)),
      size: windowSize(window),
      icon: shown.icon.flatMap { snapshot.panes.apps.icons[$0] }
    )
  }

  /// A window's title, or what stands for none.
  ///
  /// - Parameter window: The window.
  /// - Returns: The title.
  static func windowTitle(_ window: HelperWindow) -> String {
    guard let title = window.title, !title.isEmpty else { return ViewerWords.untitledWindow }
    return title
  }

  /// A window's size in points.
  ///
  /// - Parameter window: The window.
  /// - Returns: "520 × 860", or a dash when unknown.
  static func windowSize(_ window: HelperWindow) -> String {
    Words.size(ViewerStatus.Size(width: window.width, height: window.height)) ?? Words.unknown
  }
}
