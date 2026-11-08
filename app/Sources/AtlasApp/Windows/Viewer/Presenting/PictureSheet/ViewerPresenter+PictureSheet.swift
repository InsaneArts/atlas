import AtlasService
import AtlasUI
import Foundation

extension ViewerPresenter {
  /// The picture sheet over the window: what happened, the app, the time and the session on one line, the picture with
  /// a step's mark, and where it stands among the run's pictures; nil while none is open or its run is gone.
  ///
  /// - Parameters:
  ///   - moments: Every run in scope.
  ///   - snapshot: What the controller holds.
  /// - Returns: The sheet, or nil.
  static func pictureSheet(_ moments: ActivityMoments, _ snapshot: ViewerSnapshot) -> ViewerPictureSheet? {
    let facts = snapshot.panes.activity
    guard let focus = facts.sheet, let run = moments.moment(focus.run) else { return nil }
    let sequence = pictureSequence(run, everyStep: focus.everyStep)
    guard let index = sequence.firstIndex(of: pictureItem(focus.item, in: run)) else { return nil }
    let (icon, name) = runSubject(run, snapshot)
    let app = run.app.map { _ in name } ?? ""
    let who = streamName(run.stream, snapshot)
    let list = ScreenshotList(shots: snapshot.panes.shots.shots ?? [], folder: snapshot.panes.shots.folder)
    let position = ViewerWords.pictureOf(index, sequence.count)
    switch focus.item {
    case .shot(let shot):
      let listed = list.shots.first { $0.name == shot }
      let taken = listed?.takenAt ?? run.runs.first { $0.shot == shot }?.startedAt
      let picture = facts.pictures[shot] ?? snapshot.panes.thumbnails[shot]
      return ViewerPictureSheet(
        id: shot,
        icon: icon,
        title: ViewerStepWords(ViewerWords.tookScreenshot),
        line: ViewerWords.dotted([app, taken.map { clock($0, snapshot) } ?? "", who]),
        picture: picture,
        aspect: listedSize(listed) ?? pictureSize(picture) ?? shotAspect,
        position: position,
        run: ViewerWords.inRun(name),
        canGoBack: index > 0,
        canGoForward: index < sequence.count - 1,
        canShowInFinder: list.pictureFile(named: shot) != nil
      )
    case .before(let row):
      let entry = run.runs
        .flatMap(\.members)
        .first { $0.id == row }
      guard let entry, let before = entry.before else {
        return nil
      }
      let after = entry.after
      let side = after == nil ? ViewerPictureSide.before : focus.side
      let shown = side == .after ? after ?? before : before
      let at = side == .after ? afterAt(entry) : entry.facts?.at ?? ActionPicture.takenAt(before)
      let when = side == .after ? ViewerWords.justAfter : ViewerWords.justBefore
      let tone = markTone(entry, uncertain: entry.id == snapshot.reading.statusReading.uncertainRow)
      let picture = facts.pictures[shown] ?? snapshot.panes.thumbnails[shown]
      return ViewerPictureSheet(
        id: row,
        icon: icon,
        title: stepWords(entry, count: 1, in: run),
        line: ViewerWords.dotted([app, at.map { "\(clock($0, snapshot)), \(when)" } ?? "", who]),
        picture: picture,
        aspect: pictureSize(picture)
          ?? entry.facts?.display.map { CGSize(width: $0.width, height: $0.height) }
          ?? shotAspect,
        mark: actionMark(entry, repeats: 1, tone: tone),
        side: side,
        hasAfter: after != nil,
        position: position,
        run: ViewerWords.inRun(name),
        canGoBack: index > 0,
        canGoForward: index < sequence.count - 1,
        canShowInFinder: list.pictureFile(named: shown) != nil
      )
    }
  }

  /// The pictures ‹ and › step through, oldest first: a run's screenshots, or, opened from a step, every step's
  /// picture, the screen before it where one was kept and else its screenshot.
  ///
  /// - Parameters:
  ///   - run: The run.
  ///   - everyStep: Whether every step's picture counts, rather than the screenshots alone.
  /// - Returns: The pictures.
  static func pictureSequence(_ run: ActivityMoment, everyStep: Bool) -> [ViewerPictureItem] {
    run.story.compactMap { line in
      if everyStep, let pictured = line.pictured { return .before(pictured.id) }
      return line.shot.map(ViewerPictureItem.shot)
    }
  }

  /// The picture as ‹ and › know it: a step's picture by its line's first pictured row, so a picture stays found as
  /// newer repeats of the same action fold into its line.
  ///
  /// - Parameters:
  ///   - item: The picture.
  ///   - run: Its run.
  /// - Returns: The picture as the run's sequence names it.
  static func pictureItem(_ item: ViewerPictureItem, in run: ActivityMoment) -> ViewerPictureItem {
    guard case .before(let row) = item,
      let line = run.runs.first(where: { line in line.members.contains { $0.id == row } }),
      let pictured = line.pictured
    else { return item }
    return .before(pictured.id)
  }

  /// The file name of the picture the sheet shows, for Copy and Show in Finder: the screenshot, or a step's picture
  /// before or after it.
  ///
  /// - Parameters:
  ///   - moments: Every run in scope.
  ///   - snapshot: What the controller holds.
  /// - Returns: The name, or nil while no picture shows.
  static func shownPicture(_ moments: ActivityMoments, _ snapshot: ViewerSnapshot) -> String? {
    guard let focus = snapshot.panes.activity.sheet, let run = moments.moment(focus.run) else { return nil }
    switch focus.item {
    case .shot(let shot):
      return shot
    case .before(let row):
      let entry = run.runs
        .flatMap(\.members)
        .first { $0.id == row }
      guard focus.side == .after, let after = entry?.after else { return entry?.before }
      return after
    }
  }

  /// What a picture saved from the sheet is called: when it was taken, such as "Atlas picture 10.31.44", or its own
  /// name without its extension when its time is not known.
  ///
  /// - Parameters:
  ///   - name: The picture's file name.
  ///   - snapshot: What the controller holds.
  /// - Returns: The file's name.
  static func pictureFileName(_ name: String, _ snapshot: ViewerSnapshot) -> String {
    let listed = snapshot.panes.shots.shots?.first { $0.name == name }?.takenAt
    guard let taken = listed ?? ActionPicture.takenAt(name) else { return (name as NSString).deletingPathExtension }
    return ViewerWords.pictureFileName(clock(taken, snapshot))
  }

  /// Whether the picture sheet and the replay can show what the person opened: a run that is gone closes them.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: Whether each shows.
  static func shownOverlays(_ snapshot: ViewerSnapshot) -> (sheet: Bool, replay: Bool) {
    let facts = snapshot.panes.activity
    guard facts.sheet != nil || facts.replay != nil, let moments = activityMoments(snapshot) else {
      return (false, false)
    }
    return (
      facts.sheet != nil && pictureSheet(moments, snapshot) != nil,
      facts.replay != nil && replay(moments, snapshot) != nil
    )
  }

  /// When the picture after an action was taken: as the row says, else as its own name says for one Atlas took after
  /// it; nil for the next action's picture before, whose time is not the action's.
  ///
  /// - Parameter entry: The row.
  /// - Returns: The time, or nil.
  static func afterAt(_ entry: ActivityLog.Entry) -> Date? {
    guard let after = entry.after else { return nil }
    if let afterAt = entry.afterAt { return afterAt }
    return ActionPicture.isAfter(after) ? nil : ActionPicture.takenAt(after)
  }

  /// Whose work a stream is, by name: "You", or the session's.
  ///
  /// - Parameters:
  ///   - stream: The stream.
  ///   - snapshot: What the controller holds.
  /// - Returns: The name.
  static func streamName(_ stream: ActivityStream, _ snapshot: ViewerSnapshot) -> String {
    switch stream {
    case .you: ViewerWords.youGroup
    case .session(let key): resolvedSession(key, snapshot).name
    }
  }

  /// The icon of whose work a stream is; nil for the person's own.
  ///
  /// - Parameters:
  ///   - stream: The stream.
  ///   - snapshot: What the controller holds.
  /// - Returns: The icon.
  static func streamIcon(_ stream: ActivityStream, _ snapshot: ViewerSnapshot) -> RowIcon? {
    switch stream {
    case .you: nil
    case .session(let key): resolvedSession(key, snapshot).icon
    }
  }

  /// A listed screenshot's size, when the list says it.
  ///
  /// - Parameter shot: The screenshot.
  /// - Returns: Its size, or nil.
  static func listedSize(_ shot: Screenshot?) -> CGSize? {
    guard let width = shot?.width, let height = shot?.height, width > 0, height > 0 else { return nil }
    return CGSize(width: width, height: height)
  }

  /// A picture's size as read.
  ///
  /// - Parameter picture: The picture.
  /// - Returns: Its size, or nil before it is read.
  static func pictureSize(_ picture: DesktopPicture?) -> CGSize? {
    picture.map { CGSize(width: $0.image.width, height: $0.image.height) }
  }

  /// 16:10: a picture's shape before the list or the picture says it.
  static let shotAspect = CGSize(width: 16, height: 10)

  /// A time on the person's clock, to the second.
  ///
  /// - Parameters:
  ///   - date: The time.
  ///   - snapshot: What the controller holds.
  /// - Returns: "10:31:44".
  static func clock(_ date: Date, _ snapshot: ViewerSnapshot) -> String {
    Words.clockTime(date, locale: snapshot.locale, calendar: snapshot.zonedCalendar, timeZone: snapshot.timeZone)
  }

  /// The outline with the picture or the step that opened the sheet marked to take focus back as it closes, while the
  /// picture the sheet shows now is nowhere in the outline, as when ‹ and › went past a run's few pictures shown.
  ///
  /// - Parameters:
  ///   - days: The outline.
  ///   - snapshot: What the controller holds.
  /// - Returns: The outline.
  static func returningFocus(_ days: [ViewerActivityDay], _ snapshot: ViewerSnapshot) -> [ViewerActivityDay] {
    guard let opener = snapshot.panes.activity.sheet?.opener else { return days }
    let sessions = days.flatMap(\.sessions)
    let runs = sessions.flatMap(\.runs)
    let steps = runs.flatMap { $0.steps + $0.problemSteps }
    let tiles =
      sessions.flatMap { $0.strip?.tiles ?? [] } + runs.flatMap { ($0.strip?.tiles ?? []) + ($0.grid?.tiles ?? []) }
      + steps.compactMap(\.inPlace)
    guard !tiles.contains(where: \.isCurrent), !steps.contains(where: \.isCurrent) else { return days }
    var marked = days
    for day in marked.indices {
      for session in marked[day].sessions.indices {
        marked[day].sessions[session].strip?.tiles.mark(opener)
        for run in marked[day].sessions[session].runs.indices {
          var outline = marked[day].sessions[session].runs[run]
          outline.strip?.tiles.mark(opener)
          outline.grid?.tiles.mark(opener)
          outline.steps.mark(opener)
          outline.problemSteps.mark(opener)
          marked[day].sessions[session].runs[run] = outline
        }
      }
    }
    return marked
  }
}
