import AtlasService
import AtlasUI

extension ViewerPresenter {
  /// What a right click on a session's row offers: Show Activity and Show Screenshots for its work; Recover… while it
  /// needs the person, as the Helper menu offers it; Send Key, whose keys go to the helper while this window holds
  /// control; Clean Up This Session's Work… while the session left something to clean up, which asks first; and every
  /// fact about it under Details.
  ///
  /// - Parameters:
  ///   - key: The session's key; empty for Terminal.
  ///   - needsYou: Whether the session needs the person.
  ///   - facts: Every fact about it (``sessionFacts(_:record:_:)``).
  ///   - snapshot: What the controller holds.
  /// - Returns: The menu's sections.
  static func sessionMenu(key: String, needsYou: Bool, facts: [Fact], _ snapshot: ViewerSnapshot) -> [ViewerMenuSection]
  {
    let alertUp = alert(snapshot) != nil
    var sections = [
      ViewerMenuSection([
        ViewerMenuItem(ViewerWords.showActivity, command: .showActivity(session: key)),
        ViewerMenuItem(ViewerWords.showScreenshots, command: .showScreenshots(session: key)),
      ])
    ]
    if needsYou {
      sections.append(
        ViewerMenuSection([
          ViewerMenuItem(
            HelperWords.recoverItem,
            command: .askRecover,
            isEnabled: snapshot.helperMenu.canRecover && !alertUp
          )
        ])
      )
    }
    sections.append(ViewerMenuSection([sendKeys(snapshot)]))
    if let record = snapshot.panes.sessions.record(key), let cleanup = cleanup(record, snapshot) {
      sections.append(
        ViewerMenuSection([
          ViewerMenuItem(
            ViewerWords.cleanUpSession,
            command: .askCleanup(session: key),
            isEnabled: cleanup.isEnabled,
            role: .destructive
          )
        ])
      )
    }
    if !facts.isEmpty {
      sections.append(ViewerMenuSection(facts.map { .info(ViewerWords.factLine($0)) }, title: ViewerWords.details))
    }
    return sections
  }

  /// Send Key: the keys this Mac would keep for itself and those an app would keep, each sent to the helper, which
  /// can be chosen only while this window holds control, as the Helper menu's.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The item, with its submenu.
  static func sendKeys(_ snapshot: ViewerSnapshot) -> ViewerMenuItem {
    let inControl = snapshot.holdingControl
    let keys = { (keys: [HelperKey], start: Int) in
      keys.enumerated()
        .map { offset, key in
          ViewerMenuItem(key.title, command: .sendKey(index: start + offset), isEnabled: inControl)
        }
    }
    return ViewerMenuItem(
      ViewerWords.sendKeyItem,
      isEnabled: inControl,
      submenu: [
        ViewerMenuSection(keys(HelperKey.mac, .zero), title: ViewerWords.macKeys),
        ViewerMenuSection(keys(HelperKey.app, HelperKey.mac.count), title: ViewerWords.appKeys),
      ]
    )
  }

  /// A session's tooltip: its name and line, then one fact a line, then what its last cleanup did, each step a line.
  ///
  /// - Parameters:
  ///   - name: The session's name.
  ///   - line: What it last did.
  ///   - facts: Every fact about it.
  ///   - receipt: What its last cleanup did; none before one ran.
  /// - Returns: The tooltip.
  static func sessionHelp(name: String, line: String, facts: [Fact], receipt: [Fact]) -> String {
    var lines = [name]
    if !line.isEmpty { lines.append(line) }
    lines += facts.map(ViewerWords.factLine)
    if !receipt.isEmpty { lines += [ViewerWords.lastCleanup] + receipt.map(ViewerWords.factLine) }
    return lines.joined(separator: "\n")
  }
}
