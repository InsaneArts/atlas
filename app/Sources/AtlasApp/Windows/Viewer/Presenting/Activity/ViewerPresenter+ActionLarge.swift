import AtlasService
import AtlasUI
import Foundation

extension ViewerPresenter {
  /// The action shown large: what it did, who did it and in which task, which step of its moment it is, where it went
  /// by app, window and element, the screen before it and
  /// after it as taken once read (their thumbnails before), its mark over both, what Atlas knows of it, how the two are
  /// compared, the squares that changed once compared, and whether Previous and Next go anywhere. Previous and Next
  /// step through every action of its moment that kept a picture, each repeat of a folded step in turn. None once its
  /// moment or its picture is gone.
  ///
  /// - Parameters:
  ///   - moments: Every moment in scope.
  ///   - snapshot: What the controller holds.
  /// - Returns: The large view, or nil.
  static func actionLarge(_ moments: ActivityMoments, _ snapshot: ViewerSnapshot) -> ViewerActionLarge? {
    let facts = snapshot.panes.activity
    guard let focus = facts.action, let moment = moments.moment(focus.moment) else {
      return nil
    }
    let sequence = actionSequence(moment)
    guard let index = sequence.firstIndex(where: { $0.id == focus.row }), let before = sequence[index].before else {
      return nil
    }
    let entry = sequence[index]
    let tone = markTone(entry, uncertain: entry.id == snapshot.reading.statusReading.uncertainRow)
    let words = stepWords(entry, count: 1, in: moment)
    let picture = facts.pictures[before] ?? snapshot.panes.thumbnails[before]
    let after = entry.after
    let beforeAt = entry.facts?.at ?? ActionPicture.takenAt(before)
    let beforeTime = hundredths(beforeAt, snapshot)
    let afterAt = afterAt(entry)
    let afterTime = afterAt.map { hundredths($0, snapshot) }
    let agent = streamName(moment.stream, snapshot)
    let task = entry.facts?.task
    var details = [
      ViewerActionDetail(title: ViewerWords.whatDetail, value: words.text),
      ViewerActionDetail(
        title: ViewerWords.whereDetail,
        value: [entry.app ?? moment.app?.name, entry.facts?.window, entry.label ?? entry.facts?.element?.name]
          .compactMap { $0 }
          .joined(separator: " › ")
      ),
      ViewerActionDetail(
        title: task == nil ? ViewerWords.agentDetail : ViewerWords.agentAndTaskDetail,
        value: [agent, task].compactMap { $0 }
          .joined(separator: "\n")
      ),
      ViewerActionDetail(
        title: ViewerWords.outcomeDetail,
        value: ViewerWords.outcome(tone, reason: entry.facts?.reason ?? entry.reason),
        tone: tone
      ),
      ViewerActionDetail(title: ViewerWords.beforeDetail, value: ViewerWords.beforeTaken(beforeTime)),
    ]
    if let after {
      details.append(
        ViewerActionDetail(
          title: ViewerWords.afterDetail,
          value: ViewerWords.afterTaken(
            afterTime,
            ownPicture: ActionPicture.isAfter(after),
            delay: afterAt.flatMap { at in beforeAt.map { at.timeIntervalSince($0) } }
          )
        )
      )
    }
    return ViewerActionLarge(
      id: entry.id,
      words: words,
      context: ViewerWords.dotted([
        agent, task ?? "", entry.app ?? moment.app?.name ?? "", ViewerWords.stepOf(index, sequence.count),
      ]),
      icon: streamIcon(moment.stream, snapshot),
      before: picture,
      after: after.flatMap { facts.pictures[$0] ?? snapshot.panes.thumbnails[$0] },
      hasAfter: after != nil,
      aspect: picture.map { CGSize(width: $0.image.width, height: $0.image.height) }
        ?? entry.facts?.display.map { CGSize(width: $0.width, height: $0.height) }
        ?? shotAspect,
      mark: actionMark(entry, repeats: 1, tone: tone),
      beforeLabel: ViewerWords.sideLabel(ViewerWords.beforeDetail, beforeTime),
      afterLabel: after == nil ? "" : ViewerWords.sideLabel(ViewerWords.afterDetail, afterTime),
      details: details.filter { !$0.value.isEmpty },
      compare: focus.compare,
      showsChanges: focus.showsChanges,
      changes: after.flatMap { facts.changes[ViewerActivityFacts.changesKey(before: before, after: $0)] },
      canGoBack: index > 0,
      canGoForward: index < sequence.count - 1
    )
  }

  /// Which of the action shown large and the task replayed the Activity pane draws now: each only while its focus
  /// still names a moment and a picture in scope, so focus left over once they are gone means nothing.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: Whether each shows.
  static func shownOverlays(_ snapshot: ViewerSnapshot) -> (large: Bool, replay: Bool) {
    let facts = snapshot.panes.activity
    guard facts.action != nil || facts.replay != nil, let moments = activityMoments(snapshot) else {
      return (false, false)
    }
    return (
      facts.action != nil && actionLarge(moments, snapshot) != nil,
      facts.replay != nil && replay(moments, snapshot) != nil
    )
  }

  /// When an action's picture after was taken: as the service says; else, for the next step's picture before, as its
  /// name says; nil when neither says, as the name of a picture Atlas took after carries the time before.
  ///
  /// - Parameter entry: The action's row.
  /// - Returns: The time, or nil.
  static func afterAt(_ entry: ActivityLog.Entry) -> Date? {
    guard let after = entry.after else { return nil }
    if let afterAt = entry.afterAt { return afterAt }
    return ActionPicture.isAfter(after) ? nil : ActionPicture.takenAt(after)
  }

  /// Every action of a moment that kept a picture, oldest first, each repeat of a folded step on its own: what the
  /// large view's Previous and Next step through.
  ///
  /// - Parameter moment: The moment.
  /// - Returns: The actions' rows.
  static func actionSequence(_ moment: ActivityMoment) -> [ActivityLog.Entry] {
    moment.runs
      .reversed()
      .flatMap(\.members)
      .filter { $0.before != nil }
  }

  /// Whose work a moment was, by name: the person's own, or its session's.
  ///
  /// - Parameters:
  ///   - stream: Whose work it follows.
  ///   - snapshot: What the controller holds.
  /// - Returns: "You", or the session's name.
  static func streamName(_ stream: ActivityStream, _ snapshot: ViewerSnapshot) -> String {
    switch stream {
    case .you: ViewerWords.youGroup
    case .session(let key): resolvedSession(key, snapshot).name
    }
  }

  /// The icon of the agent whose work a moment was; nil for the person's own, or a session nothing names.
  ///
  /// - Parameters:
  ///   - stream: Whose work it follows.
  ///   - snapshot: What the controller holds.
  /// - Returns: The icon, or nil.
  static func streamIcon(_ stream: ActivityStream, _ snapshot: ViewerSnapshot) -> RowIcon? {
    switch stream {
    case .you: nil
    case .session(let key): resolvedSession(key, snapshot).icon
    }
  }

  /// A picture's time to the hundredth, on the person's clock.
  ///
  /// - Parameters:
  ///   - date: When it was taken; nil when unknown.
  ///   - snapshot: What the controller holds.
  /// - Returns: "10:31:09.21", or the unknown mark.
  static func hundredths(_ date: Date?, _ snapshot: ViewerSnapshot) -> String {
    guard let date else { return Words.unknown }
    return Words.clockHundredths(
      date,
      locale: snapshot.locale,
      calendar: snapshot.zonedCalendar,
      timeZone: snapshot.timeZone
    )
  }
}
