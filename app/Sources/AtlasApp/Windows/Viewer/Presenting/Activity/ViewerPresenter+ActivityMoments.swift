import AtlasService
import AtlasStatus
import AtlasUI
import Foundation

extension ViewerPresenter {
  /// The days the Activity list draws, each owning its moments, with a quiet heading before today,
  /// and under an open moment its newest steps, its last where its line ends, then "Show N more" while it holds more
  /// than it lists and the person has not asked for them all. A moment is open by default while it is the newest or
  /// ended within five minutes, the person's choice wins after that, and a moment with a step under way, the action of
  /// unknown outcome, or Problems chosen is held open.
  ///
  /// - Parameters:
  ///   - shown: The moments the filter and the search keep, newest first.
  ///   - moments: Every moment in scope, for what followed a problem.
  ///   - snapshot: What the controller holds.
  /// - Returns: The nested days.
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
      choices: facts.choices,
      now: snapshot.date
    )
    let today = snapshot.zonedCalendar.startOfDay(for: snapshot.date)
    var days: [ViewerActivityDay] = []
    var day: Date?
    for moment in shown {
      if moment.day != day {
        day = moment.day
        days.append(
          ViewerActivityDay(
            id: "\(Int(moment.day.timeIntervalSince1970))",
            title: moment.day < today ? dayHeading(moment.day, snapshot) : "",
            moments: []
          )
        )
      }
      let fold = folds[moment.id] ?? .folded
      var nested = momentHeader(moment, fold: fold, snapshot)
      if fold.isOpen {
        let (steps, more) = moment.steps(expanded: facts.expanded.contains(moment.id))
        nested.steps = steps.enumerated()
          .map { index, run in
            var step = activityStep(run, in: moment, moments: moments, snapshot)
            step.isLast = index == steps.count - 1
            return step
          }
        if more > 0 { nested.more = ViewerMoreSteps(moment: moment.id, count: more) }
      }
      days[days.count - 1].moments.append(nested)
    }
    return days
  }

  /// The quiet heading over a day before today: "Yesterday", or the day's name.
  ///
  /// - Parameters:
  ///   - day: The day's first moment.
  ///   - snapshot: What the controller holds.
  /// - Returns: The heading.
  static func dayHeading(_ day: Date, _ snapshot: ViewerSnapshot) -> String {
    let calendar = snapshot.zonedCalendar
    let yesterday = calendar.date(byAdding: .day, value: -1, to: calendar.startOfDay(for: snapshot.date))
    let title =
      day == yesterday
      ? ViewerWords.yesterdayGroup
      : Words.dayName(day, locale: snapshot.locale, calendar: snapshot.zonedCalendar, timeZone: snapshot.timeZone)
    return title
  }

  /// A moment's header: its app's icon and name, or for a moment in no app the person's mark and "You", or Commands
  /// or Screenshots; whose work it was while every session's work shows, and the person's own always; when it
  /// started, how much it held and when, and how long ago it ended.
  ///
  /// - Parameters:
  ///   - moment: The moment.
  ///   - fold: Whether it shows its steps.
  ///   - snapshot: What the controller holds.
  /// - Returns: The header.
  static func momentHeader(
    _ moment: ActivityMoment,
    fold: ActivityMoment.Fold,
    _ snapshot: ViewerSnapshot
  ) -> ViewerActivityMoment {
    let (icon, title) = momentSubject(moment, snapshot)
    let clock = { (date: Date?) in
      date.map {
        Words.clockMinute($0, locale: snapshot.locale, calendar: snapshot.zonedCalendar, timeZone: snapshot.timeZone)
      } ?? Words.unknown
    }
    let age = moment.endedAt.map { stepAge($0, snapshot) } ?? ""
    return ViewerActivityMoment(
      id: moment.id,
      icon: icon,
      title: title,
      session: momentSession(moment.stream, snapshot),
      time: clock(moment.startedAt),
      span: ViewerWords.momentSpan(
        actions: moment.actions,
        start: clock(moment.startedAt),
        end: clock(moment.endedAt)
      ),
      age: age,
      isOpen: fold.isOpen,
      canFold: fold != .held
    )
  }

  /// What leads a moment and what it is titled: its app's icon and name; the person's mark and "You" for their own
  /// actions in no app; a terminal's tile and Commands, or a camera's and Screenshots.
  ///
  /// - Parameters:
  ///   - moment: The moment.
  ///   - snapshot: What the controller holds.
  /// - Returns: The icon and the title.
  static func momentSubject(_ moment: ActivityMoment, _ snapshot: ViewerSnapshot) -> (ActivityIcon, String) {
    switch moment.subject {
    case .app(let app):
      let name = app.name ?? ViewerWords.unnamedApp
      return (.app(appIcon(bundleId: app.bundleId, name: name, snapshot.panes.icons)), name)
    case .commands where moment.stream == .you, .screenshots where moment.stream == .you:
      return (.you, ViewerWords.youGroup)
    case .commands:
      return (.symbol(.terminal), ViewerWords.commandsMoment)
    case .screenshots:
      return (.symbol(.camera), ViewerWords.screenshotsMoment)
    }
  }

  /// Whose work a moment was, as its header names it: the person's own always, and a host session's only while every
  /// session's work shows and its history names multiple agents, including sessions that have ended.
  ///
  /// - Parameters:
  ///   - stream: Whose work it follows.
  ///   - snapshot: What the controller holds.
  /// - Returns: The session, or nil.
  static func momentSession(_ stream: ActivityStream, _ snapshot: ViewerSnapshot) -> ViewerMomentSession? {
    switch stream {
    case .you:
      return ViewerMomentSession(icon: nil, name: ViewerWords.youGroup)
    case .session(let key):
      let sessions = Set(
        (snapshot.panes.activity.log?.entries ?? []).filter { $0.actor != .you }
          .map { $0.session ?? "" }
      )
      guard snapshot.shownSessions.key == nil, sessions.count > 1 else { return nil }
      let session = resolvedSession(key, snapshot)
      return ViewerMomentSession(icon: session.icon, name: session.name)
    }
  }

  /// How long ago a time was, as a step or a folded moment says it (``ViewerWords/stepAge(_:clock:)``).
  ///
  /// - Parameters:
  ///   - date: The time.
  ///   - snapshot: What the controller holds.
  /// - Returns: "now", "4 s", "3 min" or "10:31".
  static func stepAge(_ date: Date, _ snapshot: ViewerSnapshot) -> String {
    ViewerWords.stepAge(
      max(0, snapshot.date.timeIntervalSince(date)),
      clock: Words.clockMinute(
        date,
        locale: snapshot.locale,
        calendar: snapshot.zonedCalendar,
        timeZone: snapshot.timeZone
      )
    )
  }
}
