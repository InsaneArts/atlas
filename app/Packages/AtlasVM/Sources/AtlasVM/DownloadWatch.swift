import AtlasVMKit
import Foundation
import os

/// Tells a making log how a restore image's download goes, from the
/// progress of the task URLSession makes for it, fresh or resumed.
final class DownloadWatch: NSObject, URLSessionTaskDelegate {
  private let log: MakingLog
  /// What watches each task's progress, kept for as long as this lives.
  private let watching = OSAllocatedUnfairLock<[NSKeyValueObservation]>(uncheckedState: [])

  /// Telling `log`.
  init(log: MakingLog) {
    self.log = log
  }

  /// Watch the progress of `task`, made for the download.
  func urlSession(_ session: URLSession, didCreateTask task: URLSessionTask) {
    let log = log
    let observation = task.progress
      .observe(\.completedUnitCount, options: [.initial]) { progress, _ in
        log.downloading(received: progress.completedUnitCount, total: progress.totalUnitCount)
      }
    watching.withLockUnchecked { $0.append(observation) }
  }

  deinit {
    watching.withLockUnchecked { observations in
      for observation in observations {
        observation.invalidate()
      }
    }
  }
}
