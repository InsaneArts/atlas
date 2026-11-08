import AtlasService
import AtlasStatus
import AtlasUI
import Foundation

extension ViewerPresenter {
  /// 3: the most pictures a folded run or session shows beside its link; a narrow card shows fewer.
  static let stripPictures = 3
  /// 8: the pictures a run's grid shows before "Show N more".
  static let gridPictures = 8

  /// The days the outline draws, newest first: each day's sessions, newest first and at most
  /// ``ViewerWords/mostSessions`` until the person asks for the rest, each with its runs while open or its newest
  /// pictures while folded. A day's heading shows only while the outline spans more than one day. With one session
  /// chosen, its row does not show and its runs stand at the first level.
  ///
  /// - Parameters:
  ///   - shown: The runs the filter and the search keep, newest first.
  ///   - moments: Every run in scope, for what followed a problem.
  ///   - snapshot: What the controller holds.
  /// - Returns: The days.
  static func activityDays(
    _ shown: [ActivityMoment],
    moments: ActivityMoments,
    _ snapshot: ViewerSnapshot
  ) -> [ViewerActivityDay] {
    let facts = snapshot.panes.activity
    let folds = moments.folds(
      of: shown,
      filter: activityFilter(facts.filter),
      uncertainRow: snapshot.reading.statusReading.uncertainRow,
      choices: facts.choices
    )
    let groups = ActivitySessionGroup.grouped(shown)
    var order: [Date] = []
    var byDay: [Date: [ActivitySessionGroup]] = [:]
    for group in groups {
      if byDay[group.day] == nil { order.append(group.day) }
      byDay[group.day, default: []].append(group)
    }
    let chosen = snapshot.shownSessions.key != nil
    return order.map { day in
      let id = "\(Int(day.timeIntervalSince1970))"
      let name = dayTitle(day, snapshot)
      let title = order.count > 1 ? name : ""
      let all = byDay[day] ?? []
      // A session holding the run of unknown outcome is always listed, so its recovery never hides behind the rest.
      let listed =
        chosen || facts.shownDays.contains(id)
        ? all
        : all.enumerated()
          .filter { place, group in
            place < ViewerWords.mostSessions || group.runs.contains { folds[$0.id] == .held }
          }
          .map(\.element)
      let sessions = listed.map { group in
        outlineSession(group, folds: folds, moments: moments, only: groups.count == 1, chosen: chosen, snapshot)
      }
      let rest = all.count - listed.count
      return ViewerActivityDay(
        id: id,
        title: title,
        sessions: sessions,
        more: rest > 0 ? ViewerMoreSessions(day: id, title: ViewerWords.moreSessions(rest, day: name)) : nil
      )
    }
  }

  /// A day's heading: "Today", "Yesterday", or the day's name.
  ///
  /// - Parameters:
  ///   - day: The day's first moment.
  ///   - snapshot: What the controller holds.
  /// - Returns: The heading.
  static func dayTitle(_ day: Date, _ snapshot: ViewerSnapshot) -> String {
    let calendar = snapshot.zonedCalendar
    let today = calendar.startOfDay(for: snapshot.date)
    if day == today { return ViewerWords.todayGroup }
    if day == calendar.date(byAdding: .day, value: -1, to: today) { return ViewerWords.yesterdayGroup }
    return Words.dayName(day, locale: snapshot.locale, calendar: calendar, timeZone: snapshot.timeZone)
  }

  /// A session of the outline: its agent and project, what it did, its problems and when; open while it works, while
  /// it is the only one, under a filter, or as the person chose, else folded to its newest pictures; always open while
  /// one of its runs is held open.
  ///
  /// - Parameters:
  ///   - group: The session's runs on one day.
  ///   - folds: Each run's fold, by its id.
  ///   - moments: Every run in scope.
  ///   - only: Whether it is the outline's only session.
  ///   - chosen: Whether one session is chosen, so its row does not show.
  ///   - snapshot: What the controller holds.
  /// - Returns: The session.
  static func outlineSession(
    _ group: ActivitySessionGroup,
    folds: [String: ActivityMoment.Fold],
    moments: ActivityMoments,
    only: Bool,
    chosen: Bool,
    _ snapshot: ViewerSnapshot
  ) -> ViewerOutlineSession {
    let facts = snapshot.panes.activity
    let tally = group.tally
    let (icon, agent, project, working) = sessionHead(group.stream, snapshot)
    // A run held open, as its action's outcome is unknown, keeps its session open, so its recovery shows; under a
    // filter a session opens on the steps it kept.
    let held = group.runs.contains { folds[$0.id] == .held }
    let opensByDefault = working || only || facts.filter != .all
    let isOpen = chosen || held || (facts.sessionChoices[group.id] ?? opensByDefault)
    let shots = group.shots
    return ViewerOutlineSession(
      id: group.id,
      icon: icon,
      agent: agent,
      project: project,
      summary: ViewerWords.sessionSummary(tally),
      problems: ViewerWords.problems(tally.problems),
      time: span(group.startedAt, group.endedAt, running: group.isRunning, snapshot),
      isWorking: working,
      isOpen: isOpen,
      showsHead: !chosen,
      runs: isOpen || chosen
        ? group.runs.map { outlineRun($0, fold: folds[$0.id] ?? .folded, moments: moments, snapshot) } : [],
      strip: shots.isEmpty
        ? nil
        : ViewerPictureStrip(
          run: group.id,
          tiles: shots.prefix(stripPictures)
            .compactMap { pictureTile($0.shot, snapshot) },
          total: shots.count,
          opensSession: true
        )
    )
  }

  /// Who a session is: the host's icon, the agent, the project, and whether it works now; the person's own mark and
  /// "You" for their own work.
  ///
  /// - Parameters:
  ///   - stream: Whose work it is.
  ///   - snapshot: What the controller holds.
  /// - Returns: The icon, the agent, the project, and whether it works.
  static func sessionHead(_ stream: ActivityStream, _ snapshot: ViewerSnapshot)
    -> (icon: RowIcon?, agent: String, project: String, working: Bool)
  {
    switch stream {
    case .you:
      return (nil, ViewerWords.youGroup, "", false)
    case .session(let key):
      let session = resolvedSession(key, snapshot)
      let label = session.listed?.hostLabel ?? session.live?.hostLabel ?? session.record?.hostLabel ?? ""
      let folder = session.listed?.folder ?? session.live?.folder ?? session.record?.folder ?? ""
      let working = [.acting, .waiting, .needsYou].contains(session.live?.state)
      return (session.icon, label.isEmpty ? session.name : label, label.isEmpty ? "" : folder, working)
    }
  }

  /// An app run of the outline: its app, what it did there, its problems and when; open, its steps oldest first with
  /// the time only where the minute changes; folded, its problem lines and its newest pictures, or its pictures opened
  /// in place.
  ///
  /// - Parameters:
  ///   - run: The run.
  ///   - fold: Whether its steps show.
  ///   - moments: Every run in scope, for what followed a problem.
  ///   - snapshot: What the controller holds.
  /// - Returns: The run.
  static func outlineRun(
    _ run: ActivityMoment,
    fold: ActivityMoment.Fold,
    moments: ActivityMoments,
    _ snapshot: ViewerSnapshot
  ) -> ViewerOutlineRun {
    let facts = snapshot.panes.activity
    let (icon, name) = runSubject(run, snapshot)
    let tally = run.tally
    var outline = ViewerOutlineRun(
      id: run.id,
      icon: icon,
      name: name,
      summary: ViewerWords.runSummary(tally, app: run.app.map { _ in name }),
      problems: ViewerWords.problems(tally.problems),
      time: span(run.startedAt, run.endedAt, running: run.isRunning, snapshot),
      isRunning: run.isRunning,
      isOpen: fold.isOpen,
      canFold: fold != .held,
      canReplay: run.runs.contains { $0.firstBefore != nil }
    )
    if fold.isOpen {
      var minute = ""
      outline.steps = run.story.map { line in
        var step = activityStep(line, in: run, moments: moments, snapshot)
        if step.time == minute { step.time = "" } else { minute = step.time }
        return step
      }
      return outline
    }
    outline.problemSteps = run.problems.map { activityStep($0, in: run, moments: moments, snapshot) }
    let shots = run.shots.compactMap { pictureTile($0.shot, snapshot) }
    guard !shots.isEmpty else { return outline }
    if let all = facts.pictureRuns[run.id] {
      let shown = all ? shots : Array(shots.prefix(gridPictures))
      outline.grid = ViewerPictureGrid(run: run.id, tiles: shown, more: shots.count - shown.count)
    } else {
      outline.strip = ViewerPictureStrip(
        run: run.id,
        tiles: Array(shots.prefix(stripPictures)),
        total: shots.count
      )
    }
    return outline
  }

  /// What leads a run and what it is titled: its app's icon and name; the person's mark and "You" for their own
  /// actions in no app; a terminal's tile and Commands for anything else in no app.
  ///
  /// - Parameters:
  ///   - run: The run.
  ///   - snapshot: What the controller holds.
  /// - Returns: The icon and the title.
  static func runSubject(_ run: ActivityMoment, _ snapshot: ViewerSnapshot) -> (ActivityIcon, String) {
    switch run.subject {
    case .app(let app):
      let name = app.name ?? ViewerWords.unnamedApp
      return (.app(appIcon(bundleId: app.bundleId, name: name, snapshot.panes.icons)), name)
    case .commands where run.stream == .you, .screenshots where run.stream == .you:
      return (.you, ViewerWords.youGroup)
    case .commands, .screenshots:
      return (.symbol(.terminal), ViewerWords.commandsMoment)
    }
  }

  /// A screenshot as the outline frames it: its thumbnail once read, whether it shows the whole screen, and whether
  /// the sheet shows it now.
  ///
  /// - Parameters:
  ///   - name: The screenshot's file name; nil for none.
  ///   - snapshot: What the controller holds.
  /// - Returns: The picture, or nil for none.
  static func pictureTile(_ name: String?, _ snapshot: ViewerSnapshot) -> ViewerPictureTile? {
    guard let name else { return nil }
    let listed = snapshot.panes.shots.shots ?? []
    let taken = listed.first { $0.name == name }?
      .takenAt
    let time = taken.map { clock($0, snapshot) } ?? ""
    return ViewerPictureTile(
      id: name,
      thumbnail: snapshot.panes.thumbnails[name],
      fillsFrame: ScreenshotShape.fillsFrame(
        listed.first { $0.name == name },
        display: snapshot.reading.status.overview.display.map { CGSize(width: $0.width, height: $0.height) }
      ),
      isCurrent: snapshot.panes.activity.sheet?.item == .shot(name),
      label: ViewerWords.dotted([ViewerWords.screenshotWord, time])
    )
  }

  /// The time column's span on the person's clock: "01:08–now" while under way, "01:49–01:50", or "01:50".
  ///
  /// - Parameters:
  ///   - start: When it started.
  ///   - end: When it ended.
  ///   - running: Whether it is under way.
  ///   - snapshot: What the controller holds.
  /// - Returns: The span.
  static func span(_ start: Date?, _ end: Date?, running: Bool, _ snapshot: ViewerSnapshot) -> String {
    let first = start.map { minute($0, snapshot) } ?? Words.unknown
    let last = end.map { minute($0, snapshot) } ?? first
    return ViewerWords.span(first, last, running: running)
  }
}
