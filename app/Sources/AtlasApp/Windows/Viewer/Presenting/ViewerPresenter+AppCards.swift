import AtlasService
import AtlasUI
import Foundation

extension ViewerPresenter {
  /// An app's card: its icon and name, who opened it and when, or since when it has been open, its tags, its front
  /// window's picture or where its windows sit, and its windows front to back, the front one of the app in front
  /// marked.
  ///
  /// - Parameters:
  ///   - app: The app, with the windows the search keeps.
  ///   - apps: Every app open in the helper, whose windows may cover its own.
  ///   - showsOrigin: Whether its card names the session that opened it, as while every session's apps show.
  ///   - snapshot: What the controller holds.
  /// - Returns: The card.
  static func appCard(
    _ app: HelperApp,
    among apps: [HelperApp],
    showsOrigin: Bool,
    _ snapshot: ViewerSnapshot
  ) -> ViewerAppCard {
    let name = app.name.isEmpty ? ViewerWords.unnamedApp : app.name
    let origin: ViewerAppOrigin? = app.origin.flatMap { origin in
      guard showsOrigin else { return nil }
      let session = resolvedSession(origin.session, snapshot)
      return ViewerAppOrigin(icon: session.icon, name: session.name)
    }
    return ViewerAppCard(
      id: app.pid,
      name: name,
      icon: appIcon(bundleId: app.bundleId, name: name, snapshot.panes.icons),
      origin: origin,
      line: appLine(app, showsOrigin: showsOrigin, snapshot),
      tags: appTags(app),
      isFront: app.front,
      picture: appPicture(app, among: apps, snapshot),
      windows: app.windows.enumerated()
        .map { index, window in
          let title = windowTitle(window)
          return ViewerWindowRow(
            id: window.id,
            title: title,
            size: windowSize(window),
            showLabel: ViewerWords.showWindowLabel(title),
            isFront: app.front && index == 0
          )
        }
    )
  }

  /// When an app was opened, or since when it has been open: "4 min ago" beside the session that opened it,
  /// "Opened 4 min ago" while only that session's apps show, "Open since 09:12" for one already open, on the
  /// person's clock and in their time zone; nothing beside the session when it says no time.
  ///
  /// - Parameters:
  ///   - app: The app.
  ///   - showsOrigin: Whether its card names the session that opened it.
  ///   - snapshot: What the controller holds.
  /// - Returns: The line.
  static func appLine(_ app: HelperApp, showsOrigin: Bool, _ snapshot: ViewerSnapshot) -> String {
    let clock = { (date: Date) in Words.clockMinute(date, locale: snapshot.locale, timeZone: snapshot.timeZone) }
    let day = { (date: Date) in Words.dayName(date, locale: snapshot.locale, timeZone: snapshot.timeZone) }
    let today = { (date: Date) in snapshot.zonedCalendar.isDate(date, inSameDayAs: snapshot.date) }
    guard let origin = app.origin else {
      guard let launched = app.launchedAt else { return ViewerWords.alreadyOpen }
      return ViewerWords.openSince(today(launched) ? clock(launched) : day(launched))
    }
    guard let at = origin.at ?? app.launchedAt else {
      return showsOrigin ? "" : ViewerWords.openedSometime
    }
    let when = ViewerWords.openedWhen(
      seconds: snapshot.date.timeIntervalSince(at),
      clock: clock(at),
      day: today(at) ? nil : day(at)
    )
    return showsOrigin ? when : ViewerWords.opened(when)
  }

  /// An app's tags, in order: hidden, from a copy or installed by Atlas, then in front last, so as the front app
  /// changes "In front" comes and goes at the end and the tags before it never move.
  ///
  /// - Parameter app: The app.
  /// - Returns: The tags.
  static func appTags(_ app: HelperApp) -> [ViewerAppTag] {
    var tags: [ViewerAppTag] = []
    if app.hidden { tags.append(ViewerAppTag(words: ViewerWords.hidden, tone: .quiet)) }
    switch app.origin?.how {
    case .copy: tags.append(ViewerAppTag(words: ViewerWords.fromCopy, tone: .attention))
    case .install: tags.append(ViewerAppTag(words: ViewerWords.installedByAtlas, tone: .attention))
    case .launch, nil: break
    }
    if app.front { tags.append(ViewerAppTag(words: ViewerWords.inFront, tone: .accent)) }
    return tags
  }
}
