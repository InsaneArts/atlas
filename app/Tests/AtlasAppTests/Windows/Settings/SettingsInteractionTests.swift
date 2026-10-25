import AppKit
import AtlasCommandsTesting
import AtlasUI
import Testing

@testable import AtlasApp

@Suite("Settings interactions", .serialized)
struct SettingsInteractionTests {
  @Test("controller Back and Forward retain both overflowing pages while fresh visits and search reveals scroll")
  func historyPreservesPageScrollPositions() async throws {
    let projects = (0..<30)
      .map { #"{"path":"/sample/workspace/\#($0)"}"# }
      .joined(separator: ",")
    let commands = ScriptedCommands(["agents status": #"{"ok":true,"agents":[],"projects":[\#(projects)]}"#])
    let controller = SettingsController.scripted(commands: commands)
    let keyboard = SettingsSearchKeyboard(controller)
    defer { keyboard.close() }
    _ = try await keyboard.focus()
    let window = keyboard.window
    window.contentMinSize = NSSize(width: SettingsLayout.minimumWidth, height: SettingsLayout.minimumHeight / 2)
    window.setContentSize(NSSize(width: SettingsLayout.defaultWidth, height: SettingsLayout.minimumHeight / 2))
    controller.send(.navigate(.root(.helper)))
    await keyboard.settle()
    let scrolls = Self.scrollViews(in: try #require(window.contentView))
    try #require(scrolls.count > SettingsDestination.pages.count)
    let pages = Array(scrolls.dropFirst().prefix(SettingsDestination.pages.count))
    let helper = pages[try #require(SettingsDestination.pages.firstIndex(of: .root(.helper)))]
    let projectsPage = pages[try #require(SettingsDestination.pages.firstIndex(of: .projects))]
    controller.send(.navigate(.projects))
    await controller.agentsReading?.value
    await keyboard.settle()
    controller.send(.back)
    await keyboard.settle()
    for scroll in [helper, projectsPage] {
      let document = try #require(scroll.documentView)
      try #require(document.bounds.height > scroll.contentView.bounds.height)
    }
    helper.contentView.scroll(to: CGPoint(x: 0, y: SettingsLayout.minimumHeight / 4))
    helper.reflectScrolledClipView(helper.contentView)
    let helperOffset = helper.contentView.bounds.origin
    controller.send(.forward)
    await keyboard.settle()
    projectsPage.contentView.scroll(to: CGPoint(x: 0, y: SettingsLayout.minimumHeight / 4))
    projectsPage.reflectScrolledClipView(projectsPage.contentView)
    let projectsOffset = projectsPage.contentView.bounds.origin
    try #require(helperOffset.y > 0 && projectsOffset.y > 0)
    for event in [SettingsEvent.back, .forward, .back, .forward] {
      controller.send(event)
      await keyboard.settle()
      #expect(helper.contentView.bounds.origin == helperOffset)
      #expect(projectsPage.contentView.bounds.origin == projectsOffset)
      #expect(controller.state.reveal?.scrolls == false)
    }
    controller.send(.navigate(.root(.helper)))
    try await keyboard.waitForFocus { helper.contentView.bounds.minY < helperOffset.y }
    #expect(controller.state.reveal?.scrolls == true)
    controller.send(.search("workspace/0"))
    controller.send(.openSelectedSearchResult)
    try await keyboard.waitForFocus { projectsPage.contentView.bounds.minY < projectsOffset.y }
    #expect(controller.state.reveal?.scrolls == true)
  }

  /// Finds retained native page scroll views in their stable destination order.
  ///
  /// - Parameter view: The window's content or a descendant.
  /// - Returns: All nested scroll views.
  private static func scrollViews(in view: NSView) -> [NSScrollView] {
    view.subviews.flatMap { child in
      (child as? NSScrollView)
        .map { [$0] } ?? scrollViews(in: child)
    }
  }

  @Test("returning from either detail restores its disclosure and Return opens only that detail")
  func returningRestoresTheDisclosure() async throws {
    for detail in [SettingsDestination.diagnostics, .projects] {
      let commands = ScriptedCommands(["agents status": #"{"ok":true,"agents":[],"projects":[]}"#])
      let controller = SettingsController.scripted(commands: commands)
      controller.send(.navigate(.root(detail.root)))
      let keyboard = SettingsSearchKeyboard(controller)
      defer { keyboard.close() }
      let editor = try await keyboard.focus()
      controller.send(.navigate(detail))
      try await keyboard.waitForFocus {
        keyboard.window.firstResponder is NSView && keyboard.window.firstResponder !== editor
      }
      controller.send(.back)
      #expect(controller.state.reveal?.target == .disclosure(detail))
      try await keyboard.press("\r", code: 36)
      #expect(controller.state.location == detail)
      #expect(controller.state.alert == nil)
      #expect(commands.ran.allSatisfy { $0 == ["agents", "status"] })
    }
  }

  @Test("search keeps field focus on clear and disabled history cannot move its underlying page")
  func returningFromSearchRestoresFocus() async throws {
    let controller = SettingsController.scripted(commands: ScriptedCommands([:]))
    let keyboard = SettingsSearchKeyboard(controller)
    defer { keyboard.close() }
    let editor = try await keyboard.focus()
    controller.send(.search("helper"))
    try await keyboard.press("h", code: 4)
    let history = controller.session.navigation
    controller.send(.back)
    controller.send(.forward)
    #expect(controller.session.navigation == history)
    try await keyboard.press("\u{1b}", code: 53)
    #expect(!controller.state.search.isActive)
    #expect(controller.state.location == .root(.general))
    #expect(keyboard.window.firstResponder === editor)
  }

  @Test("an alert contains focus and blocks background history search and helper actions")
  func anAlertContainsInteraction() async throws {
    let commands = ScriptedCommands([:])
    let controller = SettingsController.scripted(commands: commands)
    controller.send(.navigate(.root(.helper)))
    let keyboard = SettingsSearchKeyboard(controller)
    defer { keyboard.close() }
    let editor = try await keyboard.focus()
    controller.send(.askRecover)
    keyboard.window.contentView?.layoutSubtreeIfNeeded()
    let before = controller.session.navigation
    controller.send(.back)
    controller.send(.search("helper"))
    try await keyboard.press("\t", code: 48)
    #expect(keyboard.window.firstResponder !== editor)
    #expect(controller.session.navigation == before && !controller.state.search.isActive)
    #expect(commands.ran.isEmpty)
    try await keyboard.press("\u{1b}", code: 53)
    #expect(controller.state.alert == nil && commands.ran.isEmpty)
  }

}
