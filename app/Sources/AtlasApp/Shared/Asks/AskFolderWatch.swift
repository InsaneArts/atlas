import Foundation

/// The watch on the asks folders: one dispatch source on `~/Library/Caches/Atlas`, which hears an asks folder come or
/// go, and one on each asks folder, which hears a question written, answered or taken down. A change in an asks folder
/// calls `changed`; one in the folder above only looks again for asks folders, and calls `changed` when they differ,
/// so the locks and markers every command writes there never read the questions. A folder not made yet is watched
/// once it is, when the store looks again after a reading (``rescan()``). Released, it stops.
final class AskFolderWatch {
  /// The folder the asks folders are kept in.
  private let root: URL
  /// Called on the main actor for each change in an asks folder.
  private let changed: @MainActor () -> Void
  /// The source on the folder above, while it exists.
  private var rootSource: (any DispatchSourceFileSystemObject)?
  /// The sources on each asks folder, by its path.
  private var sources: [String: any DispatchSourceFileSystemObject] = [:]

  /// Starts watching.
  ///
  /// - Parameters:
  ///   - root: The folder the asks folders are kept in.
  ///   - changed: Called on the main actor for each change in an asks folder.
  init(root: URL, changed: @escaping @MainActor () -> Void) {
    self.root = root
    self.changed = changed
    rescan()
  }

  /// Stops every source.
  deinit {
    rootSource?.cancel()
    for source in sources.values { source.cancel() }
  }

  /// Watches the folder above if it was not yet, and each asks folder there now, dropping the ones gone.
  ///
  /// - Returns: Whether the asks folders watched changed.
  @discardableResult
  func rescan() -> Bool {
    if rootSource == nil {
      rootSource = source(root.path) { [weak self] in
        if self?.rescan() == true { self?.changed() }
      }
    }
    let found = Set(AskFolder.folders(under: root).map(\.path))
    let known = Set(sources.keys)
    for gone in known.subtracting(found) {
      sources.removeValue(forKey: gone)?
        .cancel()
    }
    for path in found.subtracting(known) {
      sources[path] = source(path) { [weak self] in self?.changed() }
    }
    return found != known
  }

  /// A source on the folder at `path` that runs `event` on the main actor when an entry in it is added, removed or
  /// renamed, or the folder itself goes; nil when it can't be opened, as before it is made.
  ///
  /// - Parameters:
  ///   - path: The folder.
  ///   - event: What runs.
  /// - Returns: The running source, or nil.
  private func source(_ path: String, event: @escaping @MainActor () -> Void) -> (any DispatchSourceFileSystemObject)? {
    let descriptor = open(path, O_EVTONLY)
    guard descriptor >= 0 else { return nil }
    let source = DispatchSource.makeFileSystemObjectSource(
      fileDescriptor: descriptor,
      eventMask: [.write, .delete, .rename, .link],
      queue: .main
    )
    source.setEventHandler {
      MainActor.assumeIsolated { event() }
    }
    source.setCancelHandler { close(descriptor) }
    source.resume()
    return source
  }
}
