import AtlasService
import AtlasStatus

/// What Atlas shows outside its windows when it needs the person: a notification once per new recovery or log-out
/// (``RecoveryNotice``), withdrawn once Needs you clears, and a badge on the Dock icon while Needs you asks for the person
/// (``RecoveryNotice/needsPerson(previous:current:)``). Both follow the viewer service's answers
/// as the menu bar panel reads them; a reading that failed changes neither. Each waits for its choice in Settings,
/// read as it applies, and the badge also for the Dock icon, which shows only while a window is open (``DockIcon``).
struct AttentionSignals {
  /// Posts notifications.
  private let notifier: Notifier
  /// The Dock icon's badge.
  private let badge: DockBadge
  /// Notify me and the Dock badge, as the person set them.
  private let preferences: SettingsPreferences
  /// The service's last answer; nil until the first.
  private var lastAnswer: ViewerStatus.Overview?
  /// The service's answer before the last; nil until the second.
  private var answerBefore: ViewerStatus.Overview?
  /// Whether the Dock icon shows now.
  private var dockIconShown = false
  /// The badge shown now, so an unchanged one is not set again on every reading.
  private var shownBadge: String?
  /// Whether the last answer needed the person, so Atlas needs you is withdrawn once one no longer does.
  private var neededPerson = false

  /// Signals that have seen no answer yet, with no badge.
  ///
  /// - Parameters:
  ///   - notifier: Posts notifications.
  ///   - badge: The Dock icon's badge.
  ///   - preferences: Notify me and the Dock badge, as the person set them.
  init(notifier: Notifier, badge: DockBadge, preferences: SettingsPreferences) {
    self.notifier = notifier
    self.badge = badge
    self.preferences = preferences
  }

  /// A status reading ended: a notification if it tells something new and Notify me is on, and the badge settled.
  /// With Notify me off nothing is posted, but the answer still counts, so turning it on later never tells of a state
  /// that was already there.
  ///
  /// - Parameter status: The service's answer, or nil when the reading failed.
  mutating func read(_ status: ViewerStatus?) {
    guard let status else { return }
    let notice = RecoveryNotice.of(earlier: answerBefore, previous: lastAnswer, current: status.overview)
    answerBefore = lastAnswer
    lastAnswer = status.overview
    if let notice, preferences.loadNotify() { notifier.post(notice) }
    // A Needs you that has cleared leaves no notification behind whose Recover would act on what is over.
    let needsPerson = RecoveryNotice.needsPerson(previous: answerBefore, current: status.overview)
    if neededPerson, !needsPerson { notifier.withdraw() }
    neededPerson = needsPerson
    settleBadge()
  }

  /// A Recover started from a notification did not run: a second notification says why, unless Notify me was turned
  /// off meanwhile.
  ///
  /// - Parameter reason: Why, in plain words.
  func tell(_ reason: String) {
    if preferences.loadNotify() { notifier.postRefusal(reason) }
  }

  /// The Dock icon showed or hid.
  ///
  /// - Parameter shown: Whether it shows now.
  mutating func dockIcon(shown: Bool) {
    dockIconShown = shown
    settleBadge()
  }

  /// The person turned Notify me on or off in Settings. Turning it on asks macOS whether Atlas may notify them,
  /// which macOS asks only the first time; Atlas never asks at launch.
  ///
  /// - Parameter on: Whether it is on now.
  func notifyChanged(_ on: Bool) {
    if on { notifier.requestAuthorization() }
  }

  /// The person turned the Dock badge on or off in Settings.
  mutating func badgeChanged() {
    settleBadge()
  }

  /// Shows the badge while Needs you asks for the person, the Dock icon shows and the person asked for it; clears it
  /// otherwise.
  private mutating func settleBadge() {
    let needsYou = lastAnswer.map { RecoveryNotice.needsPerson(previous: answerBefore, current: $0) } ?? false
    let label = needsYou && dockIconShown && preferences.loadBadge() ? NotificationWords.badge : nil
    guard label != shownBadge else { return }
    shownBadge = label
    badge.show(label)
  }
}
