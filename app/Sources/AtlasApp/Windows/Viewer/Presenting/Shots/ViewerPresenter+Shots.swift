import AtlasService
import AtlasUI
import CoreGraphics
import Foundation

extension ViewerPresenter {
  /// 16 by 10: a large screenshot's shape before its picture or size is known, the screen's.
  static let shotAspect = CGSize(width: 16, height: 10)

  /// The Screenshots pane: today's count, the screenshots in their groups with the selected one ringed, or why none
  /// shows, and the one shown large. The groups are Activity's: the last hour is Now and the rest of today one group,
  /// however the day's parts would split it. Times are the person's own clock. With one host session chosen, only its
  /// screenshots list, counted by its name.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The pane.
  static func shots(_ snapshot: ViewerSnapshot) -> ViewerShotsState {
    let facts = snapshot.panes.shots
    guard let shots = facts.shots else {
      let failed = facts.failure.map {
        EmptyMessageState(headline: ViewerWords.shotsFailed, message: Words.sentence($0))
      }
      return ViewerShotsState(meta: "", days: [], empty: failed)
    }
    let rules = ShotGrouping(now: snapshot.date, calendar: snapshot.zonedCalendar)
    let days = shotDays(snapshot)
    let listed = listedShots(snapshot)
    let order =
      days
      .flatMap(\.shots)
      .map(\.name)
    let selected = selectedShot(snapshot, order: order)
    var groups: [ViewerShotDay] = []
    for day in days {
      let title = ViewerWords.shotPeriod(day.period, day: dayName(day.period, snapshot))
      let tiles = day.shots.map { shotTile($0, selected: selected, snapshot) }
      // Parts of a day that read the same, as the rest of today does, are one group under the first one's id.
      if let last = groups.indices.last, groups[last].title == title {
        groups[last].shots += tiles
      } else {
        groups.append(ViewerShotDay(id: shotDayID(day.period), title: title, shots: tiles))
      }
    }
    var meta = days.isEmpty ? "" : ViewerWords.shotsMeta(today: rules.countToday(shots))
    var empty =
      days.isEmpty ? EmptyMessageState(headline: ViewerWords.noShots, message: ViewerWords.noShotsMessage) : nil
    if let key = snapshot.shownSessions.key {
      meta = ViewerWords.shotsBy(listed.count, name: resolvedSession(key, snapshot).name)
      if days.isEmpty {
        empty = EmptyMessageState(headline: ViewerWords.noShots, message: ViewerWords.noSessionShotsMessage)
      }
    }
    return ViewerShotsState(
      meta: meta,
      days: groups,
      empty: empty,
      large: largeShot(snapshot, order: order),
      scrollRequest: facts.scrollRequest,
      heldColumns: snapshot.session.heldShotColumns
    )
  }

  /// The screenshots the pane lists, in their groups, newest first.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The groups.
  static func shotDays(_ snapshot: ViewerSnapshot) -> [ShotDay] {
    let rules = ShotGrouping(now: snapshot.date, calendar: snapshot.zonedCalendar)
    return rules.days(listedShots(snapshot), matching: "") { clock($0, snapshot) }
  }

  /// The screenshots the pane lists, newest first: every one, or with a host session chosen only those it took, as
  /// each screenshot names its session or the sessions' list names it among the session's.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The screenshots.
  static func listedShots(_ snapshot: ViewerSnapshot) -> [Screenshot] {
    let shots = snapshot.panes.shots.shots ?? []
    guard let key = snapshot.shownSessions.key else { return shots }
    let named = Set(snapshot.panes.sessions.record(key)?.screenshots ?? [])
    return shots.filter { $0.sessionKey == key || named.contains($0.name) }
  }

  /// The screenshots in the order the grid lists them, by name: what Previous and Next walk.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The names.
  static func shotOrder(_ snapshot: ViewerSnapshot) -> [String] {
    shotDays(snapshot)
      .flatMap(\.shots)
      .map(\.name)
  }

  /// The selected screenshot: the one picked while it is listed, else the first, unless Esc cleared the selection.
  ///
  /// - Parameters:
  ///   - snapshot: What the controller holds.
  ///   - order: The grid's order, when already computed.
  /// - Returns: Its name, or nil when none is listed or selected.
  static func selectedShot(_ snapshot: ViewerSnapshot, order: [String]? = nil) -> String? {
    let facts = snapshot.panes.shots
    guard !facts.selectionCleared else { return nil }
    let order = order ?? shotOrder(snapshot)
    if let picked = facts.selected, order.contains(picked) { return picked }
    return order.first
  }

  /// The screenshot shown large: the picture as taken once read, its thumbnail before; its time and size; and
  /// whether Previous, Next and Show in Finder go anywhere. None once its screenshot is no longer listed.
  ///
  /// - Parameters:
  ///   - snapshot: What the controller holds.
  ///   - order: The grid's order.
  /// - Returns: The large view, or nil.
  static func largeShot(_ snapshot: ViewerSnapshot, order: [String]) -> ViewerLargeShot? {
    let facts = snapshot.panes.shots
    guard let name = facts.large, let shot = facts.shots?.first(where: { $0.name == name }) else { return nil }
    let picture = facts.full?.picture ?? snapshot.panes.thumbnails[name]
    let size = shot.width.flatMap { width in shot.height.map { CGSize(width: width, height: $0) } }
    return ViewerLargeShot(
      id: name,
      picture: picture,
      aspect: picture.map { CGSize(width: $0.image.width, height: $0.image.height) } ?? size ?? shotAspect,
      caption: ViewerWords.shotCaption(
        time: shot.takenAt.map { clock($0, snapshot) } ?? Words.unknown,
        size: size.flatMap { Words.size(ViewerStatus.Size(width: $0.width, height: $0.height)) }
      ),
      canGoBack: ShotGrouping.step(from: name, by: -1, in: order) != nil,
      canGoForward: ShotGrouping.step(from: name, by: 1, in: order) != nil,
      canShowInFinder: facts.file(named: name) != nil
    )
  }

  /// What a screenshot dragged out is called, as a file.
  ///
  /// - Parameters:
  ///   - name: The screenshot's file name.
  ///   - snapshot: What the controller holds.
  /// - Returns: "Atlas screenshot 10.31.44", or the file name's own stem when its time is unknown.
  static func shotFileName(_ name: String, _ snapshot: ViewerSnapshot) -> String {
    guard let taken = snapshot.panes.shots.shots?.first(where: { $0.name == name })?.takenAt else {
      return (name as NSString).deletingPathExtension
    }
    return ViewerWords.shotFileName(clock(taken, snapshot))
  }

  /// A screenshot's tile, with its shape as the list says it.
  ///
  /// - Parameters:
  ///   - shot: The screenshot.
  ///   - selected: The selected screenshot's name.
  ///   - snapshot: What the controller holds.
  /// - Returns: The tile.
  static func shotTile(_ shot: Screenshot, selected: String?, _ snapshot: ViewerSnapshot) -> ViewerShotTile {
    let time = shot.takenAt.map { clock($0, snapshot) } ?? Words.unknown
    let aspect: CGFloat? =
      if let width = shot.width, let height = shot.height, width > 0, height > 0 {
        CGFloat(width) / CGFloat(height)
      } else {
        nil
      }
    return ViewerShotTile(
      id: shot.name,
      time: time,
      thumbnail: snapshot.panes.thumbnails[shot.name],
      aspect: aspect,
      isSelected: shot.name == selected,
      label: ViewerWords.shotLabel(time)
    )
  }

  /// A group's identity.
  ///
  /// - Parameter period: Its stretch of time.
  /// - Returns: An id no other group shares.
  static func shotDayID(_ period: ShotPeriod) -> String {
    switch period {
    case .lastHour: "hour"
    case .morning: "morning"
    case .afternoon: "afternoon"
    case .evening: "evening"
    case .yesterday: "yesterday"
    case .day(let start): "day-\(Int(start.timeIntervalSince1970))"
    case .undated: "undated"
    }
  }

  /// A day's name, for a group of a day before yesterday.
  ///
  /// - Parameters:
  ///   - period: The group's stretch of time.
  ///   - snapshot: What the controller holds.
  /// - Returns: "Mon 19 Oct", or empty for any other group.
  static func dayName(_ period: ShotPeriod, _ snapshot: ViewerSnapshot) -> String {
    guard case .day(let start) = period else { return "" }
    return Words.dayName(start, locale: snapshot.locale, calendar: snapshot.zonedCalendar, timeZone: snapshot.timeZone)
  }

  /// A moment on the person's clock.
  ///
  /// - Parameters:
  ///   - date: The moment.
  ///   - snapshot: What the controller holds.
  /// - Returns: "10:31:44".
  static func clock(_ date: Date, _ snapshot: ViewerSnapshot) -> String {
    Words.clockTime(date, locale: snapshot.locale, calendar: snapshot.zonedCalendar, timeZone: snapshot.timeZone)
  }
}
