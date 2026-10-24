import AtlasService
import AtlasStatus

/// What Atlas shows outside its windows when it needs the person: a notification once per new recovery or log-out
/// (``RecoveryNotice``), Atlas needs you withdrawn once Needs you clears and the logged-out one once the helper is
/// back. Both follow the viewer service's answers as the menu bar panel reads them; a reading that failed changes
/// neither. Each waits for Notify me in Settings, read as it applies.
struct AttentionSignals {
  /// Posts notifications.
  private let notifier: Notifier
  /// Notify me, as the person set it.
  private let preferences: SettingsPreferences
  /// The service's last answer; nil until the first.
  private var lastAnswer: ViewerStatus.Overview?
  /// The service's answer before the last; nil until the second.
  private var answerBefore: ViewerStatus.Overview?
  /// Whether the last answer needed the person, so Atlas needs you is withdrawn once one no longer does.
  private var neededPerson = false
  /// Whether the last answer had the helper logged out, so its notification is withdrawn once it is back.
  private var wasLoggedOut = false

  /// Signals that have seen no answer yet.
  ///
  /// - Parameters:
  ///   - notifier: Posts notifications.
  ///   - preferences: Notify me, as the person set it.
  init(notifier: Notifier, preferences: SettingsPreferences) {
    self.notifier = notifier
    self.preferences = preferences
  }

  /// A status reading ended: a notification if it tells something new and Notify me is on, and any that no longer
  /// holds withdrawn. With Notify me off nothing is posted, but the answer still counts, so turning it on later never
  /// tells of a state that was already there.
  ///
  /// - Parameter status: The service's answer, or nil when the reading failed.
  mutating func read(_ status: ViewerStatus?) {
    guard let status else { return }
    let firstAnswer = lastAnswer == nil
    let notice = RecoveryNotice.of(earlier: answerBefore, previous: lastAnswer, current: status.overview)
    answerBefore = lastAnswer
    lastAnswer = status.overview
    if let notice, preferences.loadNotify() { notifier.post(notice) }
    // A Needs you that has cleared leaves no notification behind whose Recover would act on what is over.
    let needsPerson = RecoveryNotice.needsPerson(previous: answerBefore, current: status.overview)
    if firstAnswer || neededPerson, !needsPerson { notifier.withdraw() }
    neededPerson = needsPerson
    // A helper logged in again leaves no notification behind saying it is logged out.
    let loggedOut = status.overview.mode == .loggedOut
    if firstAnswer || wasLoggedOut, !loggedOut { notifier.withdrawLoggedOut() }
    wasLoggedOut = loggedOut
  }

  /// A Recover started from a notification did not run: a second notification says why, unless Notify me was turned
  /// off meanwhile.
  ///
  /// - Parameter reason: Why, in plain words.
  func tell(_ reason: String) {
    if preferences.loadNotify() { notifier.postRefusal(reason) }
  }

  /// The person turned Notify me on or off in Settings. Turning it on asks macOS whether Atlas may notify them,
  /// which macOS asks only the first time; Atlas never asks at launch.
  ///
  /// - Parameter on: Whether it is on now.
  func notifyChanged(_ on: Bool) {
    if on { notifier.requestAuthorization() }
  }
}
