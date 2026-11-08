import AtlasService
import AtlasUI
import Foundation

extension ViewerPresenter {
  /// The task replayed: its run's steps that kept a picture, oldest first, each frame the step's marked picture
  /// before it (a folded step's first), then the screen after its last step (a folded step's last after); the pictures
  /// as taken once read, their thumbnails before. Titled by the task its agent named, else by its session's name, else
  /// by its app and when it started; with whose it was where the title does not say, how many steps and when, and the
  /// agent's icon. None once its run is gone or holds no picture.
  ///
  /// - Parameters:
  ///   - moments: Every run in scope.
  ///   - snapshot: What the controller holds.
  /// - Returns: The replay, or nil.
  static func replay(_ moments: ActivityMoments, _ snapshot: ViewerSnapshot) -> ViewerReplayState? {
    let facts = snapshot.panes.activity
    guard let focus = facts.replay, let moment = moments.moment(focus.moment) else {
      return nil
    }
    let frames = replayFrames(moment, snapshot)
    guard !frames.isEmpty else { return nil }
    let start = moment.startedAt.map { clock($0, snapshot) } ?? Words.unknown
    let end = moment.endedAt.map { clock($0, snapshot) } ?? Words.unknown
    let (title, who) = replayTitle(moment, snapshot)
    let steps = frames.filter { !$0.isAfter }
      .count
    return ViewerReplayState(
      title: title,
      detail: ViewerWords.dotted([who, ViewerWords.steps(steps), ViewerWords.span(start, end)]),
      icon: streamIcon(moment.stream, snapshot),
      frames: frames,
      index: min(focus.index, frames.count - 1),
      isPlaying: focus.isPlaying
    )
  }

  /// A replay's title and whose it was where the title does not say: the task its agent named over the session's
  /// name; the session's name alone; or, where neither is known, its app and when it started over whose it was.
  ///
  /// - Parameters:
  ///   - moment: The run.
  ///   - snapshot: What the controller holds.
  /// - Returns: The title, and whose it was, empty where the title says.
  static func replayTitle(_ moment: ActivityMoment, _ snapshot: ViewerSnapshot) -> (title: String, who: String) {
    let task = moment.runs
      .lazy
      .flatMap(\.members)
      .compactMap(\.facts?.task)
      .first
    let resolved: ViewerResolvedSession? =
      switch moment.stream {
      case .you: nil
      case .session(let key): resolvedSession(key, snapshot)
      }
    let session = resolved?.state == nil ? nil : resolved?.name
    if let task { return (task, session ?? streamName(moment.stream, snapshot)) }
    if let session { return (session, "") }
    let (_, app) = runSubject(moment, snapshot)
    let minute =
      moment.startedAt.map {
        Words.clockMinute($0, locale: snapshot.locale, calendar: snapshot.zonedCalendar, timeZone: snapshot.timeZone)
      } ?? ""
    return (ViewerWords.dotted([app, minute]), streamName(moment.stream, snapshot))
  }

  /// A run's frames, oldest first: each step that kept a picture, then the screen after the last, each with the app
  /// it was in, the screen after in the last step's.
  ///
  /// - Parameters:
  ///   - moment: The run.
  ///   - snapshot: What the controller holds.
  /// - Returns: The frames.
  static func replayFrames(_ moment: ActivityMoment, _ snapshot: ViewerSnapshot) -> [ViewerReplayFrame] {
    let full = snapshot.panes.activity.pictures
    let runs = moment.runs
      .reversed()
      .compactMap { run -> (run: ActivityRun, picture: ViewerStepPicture)? in
        guard let picture = stepPicture(run, snapshot) else { return nil }
        return (run, picture)
      }
    var frames = runs.enumerated()
      .map { index, step in
        var shot = step.picture.shot
        shot.thumbnail = full[shot.name] ?? shot.thumbnail
        let uncertain = step.run.id == snapshot.reading.statusReading.uncertainRow
        return ViewerReplayFrame(
          id: step.run.id,
          picture: shot,
          mark: step.picture.mark,
          words: stepWords(step.run, in: moment),
          detail: replayDetail(step.run, in: moment, snapshot),
          kind: uncertain ? .problem : stepKind(step.run.entry),
          stepMark: stepMark(step.run, snapshot),
          repeats: step.run.count,
          label: "\(index + 1)",
          app: step.run.entry.app ?? moment.app?.name ?? ""
        )
      }
    if let last = runs.last?.run.members.last, let after = last.after {
      var shot = activityShot(after, snapshot)
      shot.thumbnail = full[after] ?? shot.thumbnail
      frames.append(
        ViewerReplayFrame(
          id: after,
          picture: shot,
          words: ViewerStepWords(ViewerWords.screenAfter),
          detail: ViewerWords.dotted([
            moment.app?.name ?? "", afterAt(last).map { clock($0, snapshot) } ?? "",
          ]),
          label: ViewerWords.afterFrame,
          app: frames.last?.app ?? "",
          isAfter: true
        )
      )
    }
    return frames
  }

  /// The line under a replayed step's words: its app and when, and for a folded step, until when and how many.
  ///
  /// - Parameters:
  ///   - run: The step's rows.
  ///   - moment: Its app run.
  ///   - snapshot: What the controller holds.
  /// - Returns: "Chrome · 10:30:41 to 10:30:52 · 12 in one step", or "Slack · 10:31:04".
  static func replayDetail(_ run: ActivityRun, in moment: ActivityMoment, _ snapshot: ViewerSnapshot) -> String {
    let start = run.startedAt.map { clock($0, snapshot) } ?? ""
    let end = run.endedAt.map { clock($0, snapshot) } ?? start
    return ViewerWords.dotted([
      run.entry.app ?? moment.app?.name ?? "",
      ViewerWords.span(start, end),
      run.count > 1 ? ViewerWords.folded(run.count) : "",
    ])
  }

  /// A picture as a replayed step shows it: its shape as the screenshots list says it, its thumbnail once read.
  ///
  /// - Parameters:
  ///   - name: The picture's file name.
  ///   - snapshot: What the controller holds.
  /// - Returns: The picture.
  static func activityShot(_ name: String, _ snapshot: ViewerSnapshot) -> ViewerActivityShot {
    let listed = listedSize(snapshot.panes.shots.shots?.first { $0.name == name })
    return ViewerActivityShot(
      name: name,
      aspect: listed.map { $0.width / $0.height } ?? ViewerActivityShot.defaultAspect,
      thumbnail: snapshot.panes.thumbnails[name]
    )
  }
}
