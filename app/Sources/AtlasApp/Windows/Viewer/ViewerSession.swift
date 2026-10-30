import AtlasService
import AtlasUI
import CoreGraphics

/// What the main window is doing: the place shown, whose work it shows, the sidebar, watching or paused, recovering,
/// the alert asked, and its cleanups. Each is observed, so the window follows every change at once. Control, and what the
/// input method is composing, are ``ViewerControl``'s.
struct ViewerSession {
  /// The place shown: the Screen, or a list from the Library.
  var place = ViewerPlace.screen
  /// Whose work Activity and Screenshots show: every host session's, or one's.
  var chosenSession = ViewerSessionChoice.all
  /// Whether the person hid the sidebar, which the next window remembers.
  var sidebarHidden = false
  /// Whether the sidebar shows whole over the card, as the toggle or Find shows it while the window is too narrow for
  /// it.
  var overlayShown = false
  /// The window's size as last laid out; nil before the window has one.
  var windowSize: CGSize?
  /// How the content gives way to the window's size, changing only as the window crosses a threshold.
  var fit = ViewerFit()
  /// Whether the person is resizing the window now.
  var liveResizing = false
  /// How many screenshots stood to a row as the resize under way began, which the grid keeps until it ends; nil while
  /// no resize is under way.
  var heldShotColumns: Int?
  /// Whether Ended is open in the sidebar, listing every session that ended.
  var endedShown = false
  /// The sidebar's search: whether it is open, its query, and Find's asks for its focus.
  var search = ViewerSearchState()
  /// Whether the picture is paused.
  var paused = false
  /// Whether the screen is drawn at actual size, one view point a display point.
  var actualSize = false
  /// Whether this window is running recovery now.
  var recovering = false
  /// One line about the last thing that happened, until the next.
  var notice: String?
  /// The newest action when ``notice`` was said, so a newer one takes the footer's line back from it; nil when none
  /// had been done.
  var noticeAction: String?
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
  /// Where this window's cleanups of sessions' work stand, which hold Clean Up This Session's Work… while one runs.
  var cleanup = ViewerCleanupState()
  /// The screen takes the keyboard (while in control).
  var keyboardConnected = false
  /// The window is in full screen (its title bar buttons are hidden).
  var fullScreen = false
  /// Whether Check Again is reading the helper now.
  var checkingAgain = false
  /// Why Take Control was just refused, on the footer's line until it leaves by itself.
  var toast: String?
  /// Take Control's refusals counted, so each one is its own footer line and only the newest toast's wait removes it.
  var refusals = 0
  /// What the person has seen of the helper's activity: what the Activity pane showed, or the first reading's; nil
  /// before the first reading, when nothing is unseen yet.
  var activitySeen: ViewerSeenActivity?
  /// The corner the small screen keeps to, as the person last left it.
  var corner = MiniCorner.bottomTrailing
  /// What VoiceOver says about the helper's state once it has held a moment, while it asks something of the person;
  /// nil says nothing.
  var announcement: String?
}
