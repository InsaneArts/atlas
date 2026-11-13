import AtlasService
import Foundation

/// Keeps the app attached to the Atlas service for as long as it runs: attaches, passes on each line the service
/// sends, and when the service goes away attaches again, waiting a little longer each try, up to a few seconds. The
/// app never quits because its service did.
nonisolated enum ServiceAttaching {
  /// The first wait before trying again, in seconds.
  static let firstPause: TimeInterval = 0.5
  /// The longest wait between tries, in seconds.
  static let longestPause: TimeInterval = 5

  /// Attaches `connection` to the service running `service` again and again, on the calling thread, which it never
  /// leaves. A service serving other settings than `settings` is another's: this app quits, and the app started for
  /// that service takes its place.
  ///
  /// - Parameters:
  ///   - connection: The app's connection, attached and detached here.
  ///   - service: The `atlas` executable the service must run.
  ///   - settings: The settings file this app's commands use; nil for the installed one.
  ///   - received: Called on the main actor with each line the service passes on.
  ///   - ended: Called on the main actor once the service attached to went away, and awaited before the app attaches
  ///     again: what the app held of it (control, pictures) is gone with it.
  static func run(
    _ connection: ServiceConnection,
    service: URL,
    settings: String?,
    received: @escaping @MainActor @Sendable (String) -> Void,
    ended: @escaping @MainActor @Sendable () async -> Void
  ) -> Never {
    var pause = firstPause
    while true {
      do throws(ServiceError) {
        let attachment = try ServiceAttachment(service: service)
        guard let client = ViewerClient(attachment.url, appSecret: attachment.secret) else {
          throw ServiceError("Atlas's service did not say where it is.")
        }
        // Another's service: this app leaves its lock to the app started for that one, which waits for it.
        if let settings, !attachment.settings.isEmpty, attachment.settings != settings { exit(0) }
        connection.attach(client, incarnation: attachment.incarnation)
        pause = firstPause
        // The main queue keeps the lines in order; each is acknowledged once taken up.
        while let line = attachment.nextLine() {
          DispatchQueue.main.async {
            MainActor.assumeIsolated { received(line) }
            attachment.acknowledge()
          }
        }
        connection.detach()
        // Queued after the last line, and finished before the next attachment's first.
        let cleared = DispatchSemaphore(value: 0)
        Task { @MainActor in
          await ended()
          cleared.signal()
        }
        cleared.wait()
      } catch {
        // Told to the app's requests meanwhile, such as a service of another version to restart.
        connection.detach(because: error.code == "protocol" ? error : .detached)
      }
      Thread.sleep(forTimeInterval: pause)
      pause = min(pause * 2, longestPause)
    }
  }
}
