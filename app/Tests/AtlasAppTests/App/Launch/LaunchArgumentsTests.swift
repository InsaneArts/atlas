import Testing

@testable import AtlasApp

@Suite("LaunchArguments")
struct LaunchArgumentsTests {
  @Test("--notice needs a title and a message, and --switch-back names the account")
  func aNoticeNeedsItsWords() {
    #expect(
      LaunchArguments.mode(["app", "--notice", "Title", "Message", "--switch-back", "kaan"])
        == .notice(title: "Title", message: "Message", switchBack: "kaan")
    )
    #expect(
      LaunchArguments.mode(["app", "--notice", "Title", "Message"])
        == .notice(title: "Title", message: "Message", switchBack: nil)
    )
    #expect(LaunchArguments.mode(["app", "--notice", "Title"]) == .app(render: nil, dark: false), "no message")
  }

  @Test("flag modes are chosen in today's order before the app that reads standard input")
  func flagModesComeFirstInOrder() {
    #expect(
      LaunchArguments.mode(["app", "--login", "on", "--notice", "T", "M"])
        == .notice(title: "T", message: "M", switchBack: nil)
    )
    #expect(LaunchArguments.mode(["app", "--onboarding", "--login", "status"]) == .login("status"))
    #expect(LaunchArguments.mode(["app", "--render-viewer", "v", "--onboarding"]) == .onboarding)
    #expect(
      LaunchArguments.mode(["app", "--render-viewer", "v", "--render-onboarding", "o"]) == .renderOnboarding("o")
    )
    #expect(LaunchArguments.mode(["app", "--render", "p", "--render-viewer", "v"]) == .renderViewer("v"))
    #expect(LaunchArguments.mode(["app", "--render-settings", "s", "--render-viewer", "v"]) == .renderViewer("v"))
    #expect(LaunchArguments.mode(["app", "--render", "p", "--render-settings", "s"]) == .renderSettings("s"))
    #expect(LaunchArguments.mode(["app", "--render-settings", "s", "--render-menubar", "m"]) == .renderSettings("s"))
    #expect(LaunchArguments.mode(["app", "--render", "p", "--render-menubar", "m"]) == .renderMenuBar("m"))
    #expect(LaunchArguments.mode(["app", "--render", "p", "--dark"]) == .app(render: "p", dark: true))
    #expect(LaunchArguments.mode(["app"]) == .app(render: nil, dark: false))
  }
}
