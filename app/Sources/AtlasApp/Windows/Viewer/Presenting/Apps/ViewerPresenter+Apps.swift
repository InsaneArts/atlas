import AtlasControl
import AtlasService
import AtlasUI
import CoreGraphics

extension ViewerPresenter {
  /// The Apps pane: how many apps and windows are open for whose work shows, and the apps as cards. While every
  /// session's work shows, the apps agents opened come first under their own heading, then those already open; while
  /// one session is chosen, only the apps it opened, with no heading. Each group keeps its cards in the order they
  /// opened, so nothing moves as the front app changes. The sidebar's search narrows the cards; with none left, or
  /// none to list, the pane says why.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The pane.
  static func apps(_ snapshot: ViewerSnapshot) -> ViewerAppsState {
    let facts = snapshot.panes.apps
    let query = snapshot.session.search.query
    guard let apps = facts.apps else {
      let failed = facts.failure.map {
        EmptyMessageState(headline: ViewerWords.appsFailed, message: Words.sentence($0))
      }
      return ViewerAppsState(title: "", detail: "", groups: [], empty: failed)
    }
    let scope = snapshot.shownSessions
    let scoped = scopedApps(apps, scope)
    let kept = scoped.compactMap { $0.matching(query) }
      .sorted(by: opensBefore)
    let card = { (app: HelperApp) in appCard(app, showsOrigin: scope == .all, snapshot) }
    let groups: [ViewerAppGroup] =
      switch scope {
      case .one:
        kept.isEmpty ? [] : [ViewerAppGroup(id: appsSessionGroup, title: nil, cards: kept.map(card))]
      case .all:
        [
          ViewerAppGroup(
            id: appsOpenedGroup,
            title: ViewerWords.openedByAgents,
            cards: kept.filter { $0.origin != nil }
              .map(card)
          ),
          ViewerAppGroup(
            id: appsAlreadyOpenGroup,
            title: ViewerWords.alreadyOpen,
            cards: kept.filter { $0.origin == nil }
              .map(card)
          ),
        ]
        .filter { !$0.cards.isEmpty }
      }
    return ViewerAppsState(
      title: scoped.isEmpty ? "" : ViewerWords.appsOpen(scoped.count),
      detail: scoped.isEmpty ? "" : ViewerWords.windowCount(scoped.map(\.windows.count).reduce(0, +)),
      groups: groups,
      empty: groups.isEmpty ? appsEmpty(apps: apps, scoped: scoped, scope: scope, query: query) : nil
    )
  }

  /// The single group's identity while one session's apps show.
  static let appsSessionGroup = "session"
  /// The identity of the group of apps agents opened.
  static let appsOpenedGroup = "opened"
  /// The identity of the group of apps already open.
  static let appsAlreadyOpenGroup = "already"

  /// The apps whose work shows: every one while every session's work shows; only those a session opened while it is
  /// chosen, never one already open.
  ///
  /// - Parameters:
  ///   - apps: The apps open in the helper.
  ///   - scope: Whose work shows.
  /// - Returns: The apps.
  static func scopedApps(_ apps: [HelperApp], _ scope: ViewerSessionChoice) -> [HelperApp] {
    guard case .one(let key) = scope else { return apps }
    return apps.filter { $0.origin?.session == key }
  }

  /// Whether one app's card comes before another's: the one opened first, as agents opened them or as they launched,
  /// so a new app's card joins at the end and the front app changing never reorders them; by process id where neither
  /// says when.
  ///
  /// - Parameters:
  ///   - one: An app.
  ///   - other: Another.
  /// - Returns: Whether `one` comes first.
  static func opensBefore(_ one: HelperApp, _ other: HelperApp) -> Bool {
    switch (one.origin?.at ?? one.launchedAt, other.origin?.at ?? other.launchedAt) {
    case (let first?, let second?) where first != second: first < second
    case (.some, nil): true
    case (nil, .some): false
    default: one.pid < other.pid
    }
  }

  /// Why no card shows: the apps could not be read, the chosen session opened none, none is open, or the search left
  /// none.
  ///
  /// - Parameters:
  ///   - apps: The apps open in the helper.
  ///   - scoped: Those whose work shows.
  ///   - scope: Whose work shows.
  ///   - query: The sidebar's search.
  /// - Returns: The message.
  static func appsEmpty(
    apps: [HelperApp],
    scoped: [HelperApp],
    scope: ViewerSessionChoice,
    query: String
  ) -> EmptyMessageState {
    if scoped.isEmpty, case .one = scope {
      return EmptyMessageState(headline: ViewerWords.noSessionApps, message: ViewerWords.noSessionAppsMessage)
    }
    if apps.isEmpty {
      return EmptyMessageState(headline: ViewerWords.noApps, message: ViewerWords.noAppsMessage)
    }
    return EmptyMessageState(headline: ViewerWords.noAppsMatch(query), message: ViewerWords.noAppsMatchMessage)
  }

  /// The window shown from the Apps pane, outlined over the picture: its edges as shares of the helper's
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
      icon: appIcon(bundleId: shown.bundleId, name: shown.app, snapshot.panes.icons)
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
