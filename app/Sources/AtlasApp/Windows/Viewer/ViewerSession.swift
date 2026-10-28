import AtlasUI

/// What the main window is doing: the tab shown, whose work it shows, watching or paused, recovering, the alert and
/// menu open. Each is
/// observed, so the window follows every change at once. Control, and what the input method is composing, are
/// ``ViewerControl``'s.
struct ViewerSession {
  /// The tab shown.
  var tab = ViewerTab.screen
  /// Whose work Activity and Screenshots show, and Details leads with: every host session's, or one's.
  var sessionChoice = ViewerSessionChoice.all
  /// Whether the sessions menu should open once its button shows, as the panel's row for more sessions asks; it
  /// opens once, and closing the window forgets it.
  var sessionsMenuWanted = false
  /// Whether the picture is paused.
  var paused = false
  /// Whether the screen is drawn at actual size, one view point a display point.
  var actualSize = false
  /// Whether this window is running recovery now.
  var recovering = false
  /// One line about the last thing that happened, until the next.
  var notice: String?
  /// The recovery alert set aside with Not Now, until its reason changes.
  var recoveryLater: RecoveryKey?
  /// Why this window's last Recover was refused, shown in its alert until it is answered or no longer needed.
  var recoveryRefusal: String?
  /// Whether this window's Recover worked and its alert holds, saying Recovered, before it closes.
  var recoveryDone = false
  /// The alert the person asked for; nil leaves the recovery alert the helper's state calls for, if any.
  var alert: ViewerAlert?
  /// Whether the asked alert's command runs now, which holds its buttons.
  var alertRunning = false
  /// The top row's menu open now.
  var openMenu: ViewerMenu?
  /// The screen takes the keyboard (while in control).
  var keyboardConnected = false
  /// The window is in full screen (its title bar buttons are hidden).
  var fullScreen = false
  /// Whether Check Again is reading the helper now.
  var checkingAgain = false
  /// Why Take Control was just refused, in a toast until it leaves by itself.
  var toast: String?
  /// Take Control's refusals counted, so each one shakes its button once and only the newest toast's wait removes it.
  var refusals = 0
  /// What the person has seen of the helper's activity: what the Activity tab showed, or the first reading's; nil
  /// before the first reading, when nothing is unseen yet.
  var activitySeen: ViewerSeenActivity?
  /// The corner the small screen keeps to, as the person last left it.
  var corner = MiniCorner.bottomTrailing
  /// What VoiceOver says about the helper's state once it has held a moment, while it asks something of the person;
  /// nil says nothing.
  var announcement: String?
}
