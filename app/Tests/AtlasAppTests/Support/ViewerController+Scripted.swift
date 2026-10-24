import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import AtlasServiceTesting
import Foundation
import Testing

@testable import AtlasApp

extension ViewerController {
  /// A pause between readings that never ends by itself, only when the viewer cancels it to read again: a day, far
  /// past any test's limit.
  static let untilCancelled: @Sendable (Duration) async -> Void = { _ in try? await Task.sleep(for: .seconds(86_400)) }

  /// A viewer over a scripted service, reaching nothing else: its preferences in memory, a fixed clock unless given
  /// one, a heartbeat and a pause between readings as the app's unless given others, the Mac's double-click speed
  /// out of the box unless given another, no clipboard or save panel, and the helper's commands over `commands`.
  ///
  /// - Parameters:
  ///   - service: The scripted service.
  ///   - commands: The scripted command line the helper's commands run; by default one that knows none.
  ///   - rate: The saved picture rate, in seconds; 0 for none.
  ///   - clock: The time now; by default a fixed one, so pictures never age however slowly the tests run.
  ///   - heartbeat: One heartbeat tick.
  ///   - nap: The pause between readings, and before Take Control asks again.
  ///   - doubleClick: The double-click speed, in seconds.
  ///   - pictures: Where copied and saved pictures go; by default nowhere.
  ///   - switchToHelper: Stands in for locking the screen for the helper.
  ///   - actions: The helper's commands, when a test shares them with a panel; by default the viewer's own, over
  ///     `commands`, whose commands end the viewer's control first as the app's do.
  ///   - connectedAgents: Stands in for reading the agents connected to Atlas; by default none can be read.
  ///   - wallClock: The date and time now; by default a fixed one.
  ///   - shotOutlet: Where copied screenshots go and how files are shown; by default nowhere.
  /// - Returns: The viewer, closed until it is shown.
  static func scripted(
    _ service: ScriptedService,
    commands: ScriptedCommands = ScriptedCommands([:]),
    rate: Double = 0,
    clock: @escaping @MainActor () -> TimeInterval = { 0 },
    heartbeat: Duration = ViewerController.heartbeatEvery,
    nap: @escaping @Sendable (Duration) async -> Void = ViewerController.sleep,
    doubleClick: TimeInterval = 0.5,
    pictures: PictureOutlet = PictureOutlet(copy: { _ in }, save: { _ in nil }),
    switchToHelper: (@MainActor () async -> String)? = nil,
    actions: HelperActions? = nil,
    connectedAgents: @escaping @MainActor () async -> [AgentID]? = { nil },
    wallClock: @escaping @MainActor () -> Date = { Date(timeIntervalSince1970: 0) },
    shotOutlet: ShotOutlet = ShotOutlet(copy: { _ in }, reveal: { _ in })
  ) -> ViewerController {
    let viewer = WeakViewer()
    let actions =
      actions
      ?? HelperActions(commands: commands, service: service, barrier: ControlBarrier(service: service)) {
        viewer.controller?.releaseControl()
      }
    let controller = ViewerController(
      dependencies: ViewerDependencies(
        service: service,
        actions: actions,
        preferences: .memory(rate: rate),
        clock: clock,
        heartbeatInterval: heartbeat,
        nap: nap,
        doubleClickInterval: { doubleClick },
        pictures: pictures,
        switchToHelper: switchToHelper,
        connectedAgents: connectedAgents,
        wallClock: wallClock,
        shotOutlet: shotOutlet
      )
    )
    viewer.controller = controller
    return controller
  }

  /// A viewer on screen that has read its first status and picture.
  ///
  /// - Parameters:
  ///   - service: The scripted service; its picture becomes a small PNG.
  ///   - commands: The scripted command line the helper's commands run.
  ///   - rate: The saved picture rate, in seconds; 0 for none.
  ///   - clock: The time now; by default a fixed one.
  ///   - heartbeat: One heartbeat tick.
  ///   - nap: The pause between readings, and before Take Control asks again.
  ///   - doubleClick: The double-click speed, in seconds.
  ///   - pictures: Where copied and saved pictures go.
  ///   - actions: The helper's commands, when a test shares them with a panel.
  ///   - wallClock: The date and time now; by default a fixed one.
  ///   - shotOutlet: Where copied screenshots go and how files are shown; by default nowhere.
  /// - Returns: The viewer, open and watching.
  /// - Throws: The failed requirement, when the first picture never arrives.
  static func watching(
    _ service: ScriptedService,
    commands: ScriptedCommands = ScriptedCommands([:]),
    rate: Double = 0,
    clock: @escaping @MainActor () -> TimeInterval = { 0 },
    heartbeat: Duration = ViewerController.heartbeatEvery,
    nap: @escaping @Sendable (Duration) async -> Void = ViewerController.sleep,
    doubleClick: TimeInterval = 0.5,
    pictures: PictureOutlet = PictureOutlet(copy: { _ in }, save: { _ in nil }),
    actions: HelperActions? = nil,
    wallClock: @escaping @MainActor () -> Date = { Date(timeIntervalSince1970: 0) },
    shotOutlet: ShotOutlet = ShotOutlet(copy: { _ in }, reveal: { _ in })
  ) async throws -> ViewerController {
    service.picture = png()
    let viewer = scripted(
      service,
      commands: commands,
      rate: rate,
      clock: clock,
      heartbeat: heartbeat,
      nap: nap,
      doubleClick: doubleClick,
      pictures: pictures,
      actions: actions,
      wallClock: wallClock,
      shotOutlet: shotOutlet
    )
    viewer.setVisible(true)
    try await settle("the first picture never arrived") { viewer.reading.frame != nil }
    return viewer
  }
}
