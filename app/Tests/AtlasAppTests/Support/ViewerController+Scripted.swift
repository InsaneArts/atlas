import AtlasKit
import Foundation
import Testing

@testable import AtlasApp

extension ViewerController {
  /// A pause between readings that never ends by itself, only when the viewer cancels it to read again: a day, far
  /// past any test's limit.
  static let untilCancelled: @Sendable (Duration) async -> Void = { _ in try? await Task.sleep(for: .seconds(86_400)) }

  /// A viewer over a scripted service, reaching nothing else: its preferences in memory, a fixed clock unless given
  /// one, a heartbeat and a pause between readings as the app's unless given others, and no Keys menu.
  ///
  /// - Parameters:
  ///   - service: The scripted service.
  ///   - rate: The saved picture rate, in seconds; 0 for none.
  ///   - clock: The time now; by default a fixed one, so pictures never age however slowly the tests run.
  ///   - heartbeat: One heartbeat tick.
  ///   - nap: The pause between readings.
  /// - Returns: The viewer, closed until it is shown.
  static func scripted(
    _ service: ScriptedService,
    rate: Double = 0,
    clock: @escaping @MainActor () -> TimeInterval = { 0 },
    heartbeat: Duration = ViewerController.heartbeatEvery,
    nap: @escaping @Sendable (Duration) async -> Void = ViewerController.sleep
  ) -> ViewerController {
    ViewerController(
      service: service,
      preferences: .memory(rate: rate),
      clock: clock,
      heartbeatInterval: heartbeat,
      nap: nap,
      showKeys: { _ in }
    )
  }

  /// A viewer on screen that has read its first status and picture.
  ///
  /// - Parameters:
  ///   - service: The scripted service; its picture becomes a small PNG.
  ///   - rate: The saved picture rate, in seconds; 0 for none.
  ///   - clock: The time now; by default a fixed one.
  ///   - heartbeat: One heartbeat tick.
  ///   - nap: The pause between readings.
  /// - Returns: The viewer, open and watching.
  /// - Throws: The failed requirement, when the first picture never arrives.
  static func watching(
    _ service: ScriptedService,
    rate: Double = 0,
    clock: @escaping @MainActor () -> TimeInterval = { 0 },
    heartbeat: Duration = ViewerController.heartbeatEvery,
    nap: @escaping @Sendable (Duration) async -> Void = ViewerController.sleep
  ) async throws -> ViewerController {
    service.picture = png()
    let viewer = scripted(service, rate: rate, clock: clock, heartbeat: heartbeat, nap: nap)
    viewer.setVisible(true)
    try await settle("the first picture never arrived") { viewer.reading.frame != nil }
    return viewer
  }
}
