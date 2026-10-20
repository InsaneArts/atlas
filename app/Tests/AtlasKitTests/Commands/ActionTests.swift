import Testing

@testable import AtlasKit

@Suite("Action")
struct ActionTests {
  @Test(
    "each action runs its atlas command, the person's own confirmed with --yes",
    arguments: [
      (Action.switchTo("atlastest"), ["switch", "atlastest", "--yes"]),
      (.lockScreen, ["switch", "--lock", "--yes"]),
      (.listAccounts, ["switch", "--list"]),
      (.recover, ["restart"]),
      (.logOut(force: false), ["logout", "--yes"]),
      (.logOut(force: true), ["logout", "--yes", "--force"]),
      (.onboardingStatus, ["onboarding", "status"]),
      (.chooseHelper("qa"), ["onboarding", "choose", "qa"]),
      (
        .createAccount(account: "atlas", fullName: "Atlas"),
        ["onboarding", "create", "--account", "atlas", "--full-name", "Atlas"]
      ),
      (.finishSetup, ["onboarding", "finish"]),
      (.setOpensAtLogin(true), ["onboarding", "login", "on"]),
      (.setOpensAtLogin(false), ["onboarding", "login", "off"]),
      (.openMenuBar, ["menubar"]),
      (.allow(uid: 501, name: "me"), ["onboarding", "accept", "--controller-uid", "501", "--controller-name", "me"]),
      (.markWindow(open: true), ["onboarding", "window", "open"]),
      (.markWindow(open: false), ["onboarding", "window", "closed"]),
      (.agentsStatus, ["agents", "status"]),
      (.installAgents([.claude, .codex]), ["agents", "install", "--claude", "--codex"]),
      (.teachProject(path: "/Users/me/dev/Opie"), ["agents", "project", "/Users/me/dev/Opie"]),
    ] as [(Action, [String])]
  )
  func arguments(action: Action, arguments: [String]) {
    #expect(action.arguments == arguments)
  }
}
