import AppKit
import AtlasCommands

/// The one-time offer, as Atlas starts in the menu bar, to keep the home folder private from the helper, for people who
/// set Atlas up before setup asked. It reads the home folder once per launch and asks only while it is open and was
/// never offered for this helper; Make Private and Not Now both settle it. A standard system alert, with Atlas's icon.
enum HomeOffer {
  /// The default button: make the home folder private.
  private static let makePrivate = "Make Private"
  /// The way out: ask again never.
  private static let notNow = "Not Now"

  /// Reads the home folder and, while it should, asks; then does what the person chose.
  ///
  /// - Parameters:
  ///   - commands: The `atlas` command line.
  ///   - ask: Asks the person, answering whether they chose Make Private; the system alert by default.
  @MainActor static func offer(
    commands: any CommandRunning,
    ask: @MainActor (HomeFiles) -> Bool = Self.asks
  ) async {
    guard let files = HomeFiles.of(await commands.run(.homeFiles)), offers(files) else { return }
    guard ask(files) else {
      _ = await commands.run(.homeOffered)
      return
    }
    let made = await commands.run(.makeHomePrivate)
    guard !made.ok else { return }
    let failure = NSAlert()
    failure.messageText = SettingsWords.homeNotMadePrivate
    failure.informativeText = Words.sentence(made.message)
    failure.icon = AppIconImage.icon
    failure.runModal()
  }

  /// Asks the person with the system alert.
  ///
  /// - Parameter files: The home folder as Atlas read it.
  /// - Returns: Whether they chose Make Private.
  @MainActor static func asks(_ files: HomeFiles) -> Bool {
    let alert = alert(files)
    return alert.runModal() == .alertFirstButtonReturn
  }

  /// Whether to ask: the home folder is open to the helper, and keeping it private was never offered for it.
  ///
  /// - Parameter files: The home folder as Atlas read it.
  /// - Returns: Whether to ask.
  static func offers(_ files: HomeFiles) -> Bool {
    !files.private && !files.offered
  }

  /// The offer as a system alert: what the helper can open, and Make Private as the default with Not Now on Escape.
  ///
  /// - Parameter files: The home folder as Atlas read it.
  /// - Returns: The alert, not yet shown.
  @MainActor static func alert(_ files: HomeFiles) -> NSAlert {
    let alert = NSAlert()
    alert.messageText = SettingsWords.homeOfferTitle
    alert.informativeText = SettingsWords.homeOfferMessage(files)
    alert.icon = AppIconImage.icon
    alert.addButton(withTitle: makePrivate)
    let later = alert.addButton(withTitle: notNow)
    later.keyEquivalent = "\u{1b}"
    return alert
  }
}
