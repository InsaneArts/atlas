import AtlasKit
import AtlasUI

/// The Activity tab's reading: the log, and the screenshots with it while a screenshot's row can show its capture.
extension ViewerController {
  /// Reads the log, and the screenshots while the log has a screenshot's row. A reply overtaken by a newer start or
  /// stop has no effect; a failure is kept to say only while nothing was read, and a cancelled request says nothing.
  ///
  /// - Parameter number: The reading's generation.
  func readActivity(_ number: Int) async {
    do {
      let log = try await service.activity()
      // Each row's age counts back from the moment the log arrived, never from after the screenshots' reply.
      let readAt = wallClock()
      let wantsShots = log.entries.contains { $0.operation == ActivityGrouping.screenshotOperation }
      let list = wantsShots ? try? await service.screenshots() : nil
      guard number == paneWork.generation else { return }
      panes.activity.log = log
      panes.activity.readAt = readAt
      panes.activity.failure = nil
      if let list { keep(list) }
    } catch {
      guard number == paneWork.generation, !error.isCancelled else { return }
      panes.activity.failure = error.message
    }
  }
}
