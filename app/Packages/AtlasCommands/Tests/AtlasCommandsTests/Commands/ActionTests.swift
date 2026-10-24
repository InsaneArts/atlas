import AtlasServiceTesting
import Testing

@testable import AtlasCommands

@Suite("Action")
struct ActionTests {
  @Test(
    "each action runs its atlas command, the person's own confirmed with --yes",
    arguments: [
      (Action.switchTo("atlastest"), ["switch", "atlastest", "--yes"]),
      (.lockScreen, ["switch", "--lock", "--yes"]),
      (.listAccounts, ["switch", "--list"]),
      (.restart, ["restart"]),
      (.startInput, ["start-input"]),
      (.logOut(force: false), ["logout", "--yes"]),
      (.logOut(force: true), ["logout", "--yes", "--force"]),
      (.onboardingStatus, ["onboarding", "status"]),
      (.chooseHelper("qa"), ["onboarding", "choose", "qa"]),
      (
        .createAccount(account: "atlas", fullName: "Atlas"),
        ["onboarding", "create", "--account", "atlas", "--full-name", "Atlas"]
      ),
      (.forgetHelper, ["onboarding", "forget"]),
      (.closeLeftovers, ["onboarding", "close"]),
      (.closeHelper, ["onboarding", "close", "--helper"]),
      (.finishSetup, ["onboarding", "finish"]),
      (.setOpensAtLogin(true), ["onboarding", "login", "on"]),
      (.setOpensAtLogin(false), ["onboarding", "login", "off"]),
      (.openMenuBar, ["menubar"]),
      (.openAtLogin(true), ["menubar", "--login", "on"]),
      (.openAtLogin(false), ["menubar", "--login", "off"]),
      (.allow(uid: 501, name: "me"), ["onboarding", "accept", "--controller-uid", "501", "--controller-name", "me"]),
      (.markWindow(open: true), ["onboarding", "window", "open"]),
      (.markWindow(open: false), ["onboarding", "window", "closed"]),
      (.agentsStatus, ["agents", "status"]),
      (.installAgents([.claude, .codex]), ["agents", "install", "--claude", "--codex"]),
      (.connectClaudeApp, ["agents", "install", "--app"]),
      (.changeAgentParts(["claude", "claude-app"], remove: false), ["agents", "install", "--claude", "--app"]),
      (.changeAgentParts(["claude", "claude-app"], remove: true), ["agents", "remove", "--claude", "--app"]),
      (.forgetProject(path: "/Users/me/dev/Opie"), ["agents", "forget", "/Users/me/dev/Opie"]),
      (.doctor, ["doctor", "--verbose"]),
      (.checkForUpdate, ["update", "--check"]),
      (.update, ["update"]),
    ] as [(Action, [String])]
  )
  func arguments(action: Action, arguments: [String]) {
    #expect(action.arguments == arguments)
  }

  @Test(
    "Recover restarts the helper only while an action's outcome is unknown or under way, as the service's own does",
    arguments: [
      (["pending": 0, "inFlight": 0], Action.startInput),
      (["pending": 1, "inFlight": 0], .restart),
      (["pending": 0, "inFlight": 1], .restart),
    ] as [([String: Int], Action)]
  )
  func recoverMirrorsTheService(counts: [String: Int], action: Action) throws {
    let overview = try status(["overview": counts]).overview
    #expect(Action.recover(overview) == action)
  }
}
