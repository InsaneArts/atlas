import AppKit
import AtlasCommands
import AtlasOnboarding

/// The one-time offer, as Atlas starts in the menu bar, to keep the home folder private from the helper, for people who
/// set Atlas up before setup asked. Once per launch it reads setup's facts, and only once setup is finished and the
/// offer was never made for this helper does it look at the home folder, asking while that is open. Make Private and
/// Not Now both settle it, and so does a Make Private that failed, which says why. A standard system alert, with
/// Atlas's icon.
enum HomeOffer {
  /// The default button: make the home folder private.
  private static let makePrivate = "Make Private"
  /// The way out: ask again never.
  private static let notNow = "Not Now"

  /// Reads setup's facts and the home folder and, while it should, asks; then does what the person chose.
  ///
  /// - Parameters:
  ///   - commands: The `atlas` command line.
  ///   - ask: Asks the person, answering whether they chose Make Private; the system alert by default.
  ///   - tell: Says why Make Private failed; the system alert by default.
  @MainActor static func offer(
    commands: any CommandRunning,
    ask: @MainActor (HomeFiles) -> Bool = Self.asks,
    tell: @MainActor (String) -> Void = Self.tells
  ) async {
    let status = await commands.run(.onboardingStatus)
    guard status.ok, let facts = OnboardingFacts(json: status.reply), due(facts),
      let files = HomeFiles.of(await commands.run(.homeFiles)), offers(files)
    else { return }
    guard ask(files) else {
      _ = await commands.run(.homeOffered)
      return
    }
    let made = await commands.run(.makeHomePrivate)
    guard !made.ok else { return }
    // Asked and answered: a failure settles it too, and says why once.
    _ = await commands.run(.homeOffered)
    tell(Words.sentence(made.message))
  }

  /// Says why Make Private failed, with the system alert.
  ///
  /// - Parameter message: What the command said.
  @MainActor static func tells(_ message: String) {
    let failure = NSAlert()
    failure.messageText = SettingsWords.homeNotMadePrivate
    failure.informativeText = message
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

  /// Whether the home folder is worth a look: setup is finished, and the offer was never made for this helper.
  ///
  /// - Parameter facts: What setup's status said.
  /// - Returns: Whether to look.
  static func due(_ facts: OnboardingFacts) -> Bool {
    facts.finished && !facts.homePrivacyOffered
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
