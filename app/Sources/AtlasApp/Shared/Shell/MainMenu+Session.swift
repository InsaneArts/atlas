import AppKit

extension MainMenu {
  /// All Sessions (⌥⌘0), Previous Session (⌥⌘↑) and Next Session (⌥⌘↓), in the sidebar's order; Show Ended Sessions,
  /// ticked while Ended lists every session that ended; then Clean Up This Session's Work… for the session chosen,
  /// which asks first.
  ///
  /// - Returns: The menu.
  static func session() -> NSMenu {
    let session = NSMenu(title: ViewerWords.sessionMenu)
    let all = session.addItem(
      withTitle: ViewerWords.allSessionsItem,
      action: #selector(ViewerMenuActions.chooseAllSessions(_:)),
      keyEquivalent: "0"
    )
    all.keyEquivalentModifierMask = [.command, .option]
    let previous = session.addItem(
      withTitle: ViewerWords.previousSessionItem,
      action: #selector(ViewerMenuActions.choosePreviousSession(_:)),
      keyEquivalent: arrow(NSUpArrowFunctionKey)
    )
    previous.keyEquivalentModifierMask = [.command, .option]
    let next = session.addItem(
      withTitle: ViewerWords.nextSessionItem,
      action: #selector(ViewerMenuActions.chooseNextSession(_:)),
      keyEquivalent: arrow(NSDownArrowFunctionKey)
    )
    next.keyEquivalentModifierMask = [.command, .option]
    session.addItem(.separator())
    session.addItem(
      withTitle: ViewerWords.showEndedItem,
      action: #selector(ViewerMenuActions.toggleEnded(_:)),
      keyEquivalent: ""
    )
    session.addItem(.separator())
    session.addItem(
      withTitle: ViewerWords.cleanUpSession,
      action: #selector(ViewerMenuActions.askCleanupChosen(_:)),
      keyEquivalent: ""
    )
    return session
  }

  /// An arrow key as a menu item's key equivalent.
  ///
  /// - Parameter key: The arrow's function key, such as `NSUpArrowFunctionKey`.
  /// - Returns: The key equivalent.
  private static func arrow(_ key: Int) -> String {
    UnicodeScalar(key)
      .map { String(Character($0)) } ?? ""
  }
}
