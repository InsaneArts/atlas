import AtlasOnboarding
import AtlasOnboardingTesting
import AtlasUI
import Foundation

@testable import AtlasApp

/// The setup window's sample facts and snapshots, from either side, for the presenter's suites.
enum OnboardingSamples {
  /// The person's account, with a helper as `helper` describes it, or none.
  ///
  /// - Parameters:
  ///   - helper: The helper's side of the reply, or nil before one is chosen.
  ///   - finished: Whether setup has finished.
  /// - Returns: The facts.
  /// - Throws: When the reply is not a status.
  static func person(_ helper: [String: Any]?, finished: Bool = false) throws -> OnboardingFacts {
    var reply: [String: Any] = [
      "role": "controller", "me": ["name": "me"], "app": ["installed": true, "signed": true],
      "configured": helper != nil, "finished": finished,
      "candidates": [["name": "testaccount", "realName": "Test Account"]],
    ]
    if let helper { reply["helper"] = helper }
    return try facts(reply)
  }

  /// The helper `atlas`, chosen and paired, in the state given.
  ///
  /// - Parameters:
  ///   - signedIn: Whether it is logged in.
  ///   - answering: Whether Atlas answers in it.
  ///   - hidden: Whether its session is off screen.
  ///   - allowed: Whether both permissions are on.
  /// - Returns: The helper's side of the reply.
  static func helper(
    signedIn: Bool = true,
    answering: Bool = true,
    hidden: Bool = true,
    allowed: Bool = true
  ) -> [String: Any] {
    [
      "exists": true, "name": "atlas", "realName": "Atlas", "administrator": false, "paired": true,
      "signedIn": signedIn, "answering": answering, "onConsole": !hidden, "hidden": hidden,
      "permissions": ["screenCapture": allowed, "accessibility": allowed],
    ]
  }

  /// The helper's account, asked by `asker` (none when empty), with Atlas running as given.
  ///
  /// - Parameters:
  ///   - asker: The asking account's short name; its full name is Ismayil.
  ///   - running: Whether Atlas's job runs here.
  ///   - accessibility: Whether Accessibility is on.
  ///   - screen: Whether Screen Recording is on.
  /// - Returns: The facts.
  /// - Throws: When the reply is not a status.
  static func helperSide(
    asker: String = "theiskaa",
    running: Bool = true,
    accessibility: Bool = false,
    screen: Bool = false
  ) throws -> OnboardingFacts {
    var controller: [String: Any] = ["uid": 501, "name": asker]
    if !asker.isEmpty { controller["realName"] = "Ismayil" }
    return try facts([
      "role": "helper", "me": ["name": "atlas", "realName": "Atlas"], "app": ["installed": true, "signed": true],
      "helper": [
        "controller": controller, "installed": running, "running": running,
        "permissions": ["screenCapture": screen, "accessibility": accessibility],
      ],
    ])
  }

  /// macOS 26, whose System Settings names the permissions the tests expect.
  static let tahoe = OperatingSystemVersion(majorVersion: 26, minorVersion: 0, patchVersion: 0)

  /// A snapshot that has read `facts` once.
  ///
  /// - Parameters:
  ///   - facts: The facts read.
  ///   - choices: The person's choices.
  /// - Returns: The snapshot.
  static func snapshot(_ facts: OnboardingFacts, _ choices: OnboardingChoices = .init()) -> OnboardingSnapshot {
    OnboardingSnapshot(facts: facts, choices: choices, loaded: true, isOnConsole: true)
  }

  /// Choices past the welcome, on the form when `creating`.
  ///
  /// - Parameters:
  ///   - creating: Whether the person is on the account form.
  ///   - agentsDone: Whether the agents screen is behind them.
  /// - Returns: The choices.
  static func choices(creating: Bool = false, agentsDone: Bool = false) -> OnboardingChoices {
    var choices = OnboardingChoices()
    choices.started = true
    choices.creating = creating
    choices.agentsDone = agentsDone
    return choices
  }

  /// Which screen a state shows, if it shows one.
  ///
  /// - Parameter state: The window's state.
  /// - Returns: The screen's id, or nil for the first reading or the permission panel.
  static func screen(_ state: OnboardingState) -> OnboardingScreen.ID? {
    if case .screen(let screen) = state.content { return screen.id }
    return nil
  }
}
