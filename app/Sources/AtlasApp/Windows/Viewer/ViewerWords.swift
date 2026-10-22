import AtlasKit
import AtlasUI

/// The main window's words that depend on its state: the top row's buttons and tooltips, the tabs, the empty
/// messages, Details' rows, the alerts, the menus and every notice.
///
/// The rule every window follows: fixed copy stays in AtlasUI's views; a presenter supplies only the words that
/// depend on state, interpolated, chosen by a condition or derived from facts, and a list row's fixed name travels
/// with its value. Words the window shares with another are AtlasKit's ``Words`` or the app's ``HelperWords``.
enum ViewerWords {
  /// The helper's name when the status gives none.
  static let helper = "Helper"
  /// The window's title, which only the Window menu shows.
  static let windowTitle = "Atlas"

  // MARK: The top row

  /// The control button while watching.
  static let takeControl = "Take Control"
  /// The control button while watching, in the narrowest window.
  static let takeControlShort = "Take"
  /// The control button in control.
  static let giveBack = "Give Back"
  /// The control button while control is asked for.
  static let connectingControl = "Connecting…"
  /// The control button while the helper needs the person.
  static let recoverButton = "Recover…"
  /// The control button's tooltip while watching.
  static let takeControlHelp = "Use your mouse and keyboard in the helper (\(KeyMap.releaseSymbols))"
  /// The control button's tooltip in control.
  static let giveBackHelp = "Give control back (\(KeyMap.releaseSymbols))"
  /// The control button's tooltip while the helper needs the person.
  static let recoverHelp = "Check the helper and start its mouse and keyboard again"
  /// The pause button, for VoiceOver, while the picture is paused.
  static let resumePicture = "Resume picture"
  /// The pause button, for VoiceOver, while pictures arrive.
  static let pausePicture = "Pause picture"
  /// The pause button's tooltip while the picture is paused.
  static let resumePictureHelp = "Resume Picture (⇧⌘P)"
  /// The pause button's tooltip while pictures arrive.
  static let pausePictureHelp = "Pause Picture (⇧⌘P)"
  /// ⋯, for VoiceOver.
  static let more = "More"
  /// ⋯'s tooltip.
  static let moreHelp = "The picture and the helper"
  /// The Details button, for VoiceOver, while Details shows.
  static let hideDetails = "Hide details"
  /// The Details button, for VoiceOver, while it is hidden.
  static let showDetails = "Show details"
  /// The Details button's tooltip while Details shows, and the View menu's item.
  static let hideDetailsHelp = "Hide Details (⌥⌘I)"
  /// The Details button's tooltip while it is hidden.
  static let showDetailsHelp = "Show Details (⌥⌘I)"
  /// The tabs, for VoiceOver.
  static let tabsLabel = "What you see"

  // MARK: The main menu

  /// The View menu's item while Details is hidden.
  static let showDetailsItem = "Show Details"
  /// The View menu's item while Details shows.
  static let hideDetailsItem = "Hide Details"
  /// The View menu's item while pictures arrive.
  static let pausePictureItem = "Pause Picture"
  /// The View menu's item while the picture is paused.
  static let resumePictureItem = "Resume Picture"
  /// The View menu's actual size item.
  static let actualSizeItem = "Actual Size"
  /// The View menu's full screen item, which the system renames while in full screen.
  static let fullScreenItem = "Enter Full Screen"
  /// The Helper menu's item while an agent may act.
  static let stopAgentItem = "Stop Agent"
  /// Stop's tooltip, with its shortcut.
  static let stopAgentHelp = "Stop what the agent is doing in the helper, until you resume (⌘.)"
  /// Resume's tooltip, with its shortcut.
  static let resumeAgentHelp = "Let agents act in the helper again (⌘.)"
  /// The Helper menu's item once agents are stopped.
  static let resumeAgentItem = "Resume Agent"

  /// A tab's word, in the top row and the View menu.
  ///
  /// - Parameter tab: The tab.
  /// - Returns: "Screen", "Activity", "Apps" or "Screenshots".
  static func tab(_ tab: ViewerTab) -> String {
    switch tab {
    case .screen: "Screen"
    case .activity: "Activity"
    case .apps: "Apps"
    case .screenshots: "Screenshots"
    }
  }

  // MARK: The screen

  /// Connecting's headline.
  static let connectingHeadline = "Connecting to the helper…"
  /// Connecting's sentence.
  static let connectingMessage = "This takes a moment after the Mac starts."
  /// A logged-out helper's headline.
  static let loggedOutHeadline = "The helper is logged out"
  /// A logged-out helper's sentence.
  static let loggedOutMessage = "Agents can't use it until you log in to it again."
  /// Not ready's headline.
  static let notReadyHeadline = "Atlas isn't answering in the helper"
  /// Not ready's sentence.
  static let notReadyMessage = "Recover checks the helper and starts it again."
  /// A picture paused before the first one arrived: the headline.
  static let pausedHeadline = "Picture paused"
  /// A picture paused before the first one arrived: the sentence.
  static let pausedMessage = "Work in the helper goes on."
  // MARK: Details

  /// The account row's name.
  static let accountRow = "Account"
  /// The state row's name.
  static let stateRow = "State"
  /// The login time row's name.
  static let loggedInRow = "Logged in"
  /// The connected agents row's name.
  static let agentsRow = "Agents"
  /// Ready to act's one row while every check passes.
  static let readyToAct = "Ready to act"
  /// That row's value.
  static let allChecksPass = "All checks pass"

  /// The quiet count of the checks that pass, under those that do not.
  ///
  /// - Parameter count: How many pass.
  /// - Returns: "1 other passes" or "2 others pass".
  static func othersPass(_ count: Int) -> String {
    count == 1 ? "1 other passes" : "\(count) others pass"
  }

  /// No agent is connected to Atlas yet.
  static let noAgents = "None"

  // MARK: The picture

  /// Pictures as often as the window reads them.
  static let live = "Live"
  /// A picture every five seconds.
  static let everyFiveSeconds = "Every 5 seconds"
  /// No new pictures.
  static let paused = "Paused"
  /// The ⋯ menu's picture header.
  static let pictureHeader = "Picture"
  /// Copy Picture.
  static let copyPicture = "Copy Picture"
  /// Save Picture….
  static let savePicture = "Save Picture…"
  /// Once the picture is on the clipboard.
  static let pictureCopied = "Picture copied."
  /// A picture that could not be written.
  static let pictureNotSaved = "Atlas couldn't save the picture."

  /// The ⋯ menu's note: the helper and its display.
  ///
  /// - Parameters:
  ///   - account: The helper as people know it.
  ///   - display: Its display, when known.
  /// - Returns: "Helper “Atlas” · 2560 × 1440".
  static func helperNote(account: String, display: String?) -> String {
    let name = "Helper “\(account)”"
    return display.map { "\(name) · \($0)" } ?? name
  }

  // MARK: Keys

  /// The Keys menu's first header.
  static let macKeys = "Keys your Mac would keep"
  /// The Keys menu's second header.
  static let appKeys = "Keys an app would keep"

  // MARK: Alerts

  /// The way out of an alert.
  static let cancel = "Cancel"
  /// The recovery alert's way out, which sets it aside.
  static let notNow = "Not Now"
  /// The recovery alert's title.
  static let recoverTitle = "Recover the helper?"
  /// The recovery alert's sentence while an action's outcome is unknown.
  static let recoverUnfinished =
    "An action may not have finished. Atlas checks the helper and starts its mouse and keyboard again."
  /// The recovery alert's sentence while the helper's input stopped.
  static let recoverInput = "Mouse and keyboard stopped in the helper. Atlas checks the helper and starts them again."
  /// The recovery alert's sentence when it was asked for otherwise.
  static let recoverAsked = "Atlas checks the helper and starts it again. No action runs twice."
  /// The recovery alert's button.
  static let recover = "Recover"
  /// The recovery alert's button while recovery runs.
  static let recovering = "Recovering…"
  /// The switch alert's title.
  static let switchTitle = "Switch to the helper?"
  /// The switch alert's sentence.
  static let switchMessage =
    "Your screen locks so you can log in to the helper. Everything here keeps running, and you can switch back any time."
  /// The switch alert's button.
  static let switchButton = "Switch"
  /// The switch alert's button while it runs.
  static let switching = "Switching…"
  /// The log-out alert's title.
  static let logOutTitle = "Log out the helper?"
  /// The log-out alert's sentence.
  static let logOutMessage = "Its apps quit, and agents can't use it until you log in to it again."
  /// The log-out alert's button.
  static let logOutButton = "Log Out"
  /// The log-out alert's button while it runs.
  static let loggingOut = "Logging Out…"
  /// The forced log-out alert's title, after an app held a log-out up.
  static let logOutHeldTitle = "The helper didn't log out"
  /// The forced log-out alert's sentence.
  static let logOutHeldMessage =
    "An app in the helper is holding it up. Log Out Anyway quits it, and anything unsaved there is lost."
  /// The forced log-out alert's button.
  static let logOutAnyway = "Log Out Anyway"
  /// The restart alert's title.
  static let restartTitle = "Restart the helper?"
  /// The restart alert's sentence.
  static let restartMessage = "Atlas starts its service in the helper again. Anything an agent is doing there stops."
  /// The restart alert's button.
  static let restartButton = "Restart"
  /// The restart alert's button while it runs.
  static let restarting = "Restarting…"

  // MARK: Notices

  /// Once Stop ran.
  static let agentStopped = "Agent stopped."
  /// Once Resume ran.
  static let agentResumed = "Agent resumed."
  /// Take Control still refused after its one retry: an agent's action holds the helper.
  static let agentMidAction = "The agent is mid-action. Try again in a moment."
  /// Once the helper logged out.
  static let helperLoggedOut = "The helper is logged out."
  /// Once the helper restarted.
  static let helperRestarted = "The helper was restarted. Take control again when ready."
  /// The helper's session or display changed, which ended control.
  static let helperChanged = "The helper changed. Take control again when ready."
  /// The status said input is not ready, which ended control, when it gives no reason.
  static let notReady = "The helper isn't ready for mouse and keyboard."
  /// The service said no viewer holds control any more: it was released from the menu bar, say.
  static let releasedElsewhere = "Control ended outside this window. Take control again when ready."
  /// A stale picture ended control.
  static let stalePicture = "The picture stopped updating. Wait for a fresh one, then take control again."
  /// Typed text held a grapheme longer than the helper takes in one go.
  static let sequenceTooLong = "This text contains a character sequence too long to type."

  /// A paste longer than the helper takes at once.
  ///
  /// - Parameter limit: The most Unicode characters one paste may carry.
  /// - Returns: "Paste is limited to 2,048 characters at a time; an emoji or an accent can count as more than one."
  static func pasteTooLong(_ limit: Int) -> String {
    "Paste is limited to \(limit.formatted()) characters at a time; an emoji or an accent can count as more than one."
  }

  /// Control ended because the status said input is not ready, in the status's own words.
  ///
  /// - Parameter reason: The status's reason; empty, or the placeholder before any reason, for none.
  /// - Returns: "Control ended. Mouse and keyboard stopped in the helper. Recover starts them again."
  static func controlEnded(_ reason: String) -> String {
    let known = reason.isEmpty || reason == Words.connecting ? notReady : reason
    return "Control ended. \(Words.sentence(known))"
  }
}
