import AtlasService
import AtlasUI

extension ViewerPresenter {
  /// The Library at the sidebar's foot: Activity, Screenshots and Apps, in the View menu's order, each with its word and
  /// shortcut, how many its list holds once read (``count(of:_:)``), the one showing chosen, and Activity's dot while it
  /// holds actions the person has not seen.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The rows.
  static func library(_ snapshot: ViewerSnapshot) -> [ViewerLibraryRow] {
    ViewerPlace.keptPlaces
      .map { place in
        ViewerLibraryRow(
          place: place,
          title: ViewerWords.place(place),
          help: ShortcutWords.place(place),
          count: count(of: place, snapshot),
          unseen: place == .activity ? unseen(snapshot) : .none,
          isChosen: snapshot.session.place == place
        )
      }
  }

  /// How many a Library list holds, as its pane counts them for whose work shows: today's actions in Activity, the
  /// screenshots Screenshots lists, and the apps open in the helper, only those it opened while a session is chosen;
  /// nil until the list has been read.
  ///
  /// - Parameters:
  ///   - place: Activity, Screenshots or Apps.
  ///   - snapshot: What the controller holds.
  /// - Returns: The count, or nil.
  static func count(of place: ViewerPlace, _ snapshot: ViewerSnapshot) -> Int? {
    let panes = snapshot.panes
    switch place {
    case .activity:
      guard let log = panes.activity.log, let readAt = panes.activity.readAt else { return nil }
      let rules = ActivityGrouping(log: log, readAt: readAt, now: snapshot.date, calendar: snapshot.zonedCalendar)
      return rules.countToday(in: ActivityScope(snapshot.shownSessions))
    case .screenshots:
      return panes.shots.shots.map { _ in listedShots(snapshot).count }
    case .apps:
      return panes.apps.apps.map { scopedApps($0, snapshot.shownSessions).count }
    case .screen:
      return nil
    }
  }

  /// What Activity holds unseen: the actions newer than every one seen, orange once one of them failed, even after it
  /// left the status's few, but never for one refused before anything was sent; nothing while Activity shows or before
  /// the first reading.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: What is unseen.
  static func unseen(_ snapshot: ViewerSnapshot) -> ViewerUnseen {
    guard snapshot.session.place != .activity, let seen = snapshot.session.activitySeen else { return .none }
    let unseen = seen.unseen(in: snapshot.reading.statusReading.activity(limit: .max))
    if seen.failed || unseen.contains(where: \.failed) { return .failed }
    return unseen.isEmpty ? .none : .new
  }
}
