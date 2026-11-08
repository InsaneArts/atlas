import AppKit
import SystemConfiguration

/// The live ``SetupWatching``: what macOS says when something the setup window waits for may have changed. This login
/// coming back to the screen or leaving it, the account on the Mac's screen changing (the helper logging in, a switch
/// either way), Atlas coming to the front (as from System Settings), Accessibility allowed or taken back, and a change
/// in a watched folder (the acceptance Allow writes, the helper's job starting). It only says when; the window's
/// reading says what changed. The console user's watch holds it until ``stop()``.
@MainActor
final class SetupChanges: NSObject, SetupWatching {
  /// The distributed notification macOS posts when an app is allowed Accessibility, or no longer is.
  static let accessibilityChanged = Notification.Name("com.apple.accessibility.api")

  /// One element for each moment something may have changed, the newest kept while the window is busy.
  let changes: AsyncStream<Void>
  /// Where the changes are told.
  private let sink: AsyncStream<Void>.Continuation
  /// The dynamic store whose console user key is watched, and the run loop source its changes arrive on.
  private var console: (store: SCDynamicStore, source: CFRunLoopSource)?
  /// The folders asked to be watched.
  private var folders: [String] = []
  /// One source per folder watched now: the folder itself, or the nearest one above it that exists.
  private var sources: [any DispatchSourceFileSystemObject] = []
  private var stopped = false

  /// Starts every watch but the folders', which ``watch(folders:)`` names.
  override init() {
    (changes, sink) = AsyncStream.makeStream(of: Void.self, bufferingPolicy: .bufferingNewest(1))
    super.init()
    let workspace = NSWorkspace.shared.notificationCenter
    for name in [NSWorkspace.sessionDidBecomeActiveNotification, NSWorkspace.sessionDidResignActiveNotification] {
      workspace.addObserver(self, selector: #selector(changed), name: name, object: nil)
    }
    NotificationCenter.default
      .addObserver(self, selector: #selector(changed), name: NSApplication.didBecomeActiveNotification, object: nil)
    // Delivered at once: the person allows Atlas in System Settings, so Atlas is not the active app then.
    DistributedNotificationCenter.default()
      .addObserver(
        self,
        selector: #selector(changed),
        name: Self.accessibilityChanged,
        object: nil,
        suspensionBehavior: .deliverImmediately
      )
    console = followConsoleUser()
  }

  /// Watches `folders` too, in place of those watched before.
  ///
  /// - Parameter folders: The folders' paths.
  func watch(folders: [String]) {
    guard !stopped else { return }
    // Asked again with the same folders, they are watched anew: a watch lost meanwhile comes back.
    self.folders = folders
    rearm()
  }

  /// Stops every watch and ends ``changes``.
  func stop() {
    guard !stopped else { return }
    stopped = true
    NSWorkspace.shared.notificationCenter
      .removeObserver(self)
    NotificationCenter.default
      .removeObserver(self)
    DistributedNotificationCenter.default()
      .removeObserver(self)
    if let console { CFRunLoopSourceInvalidate(console.source) }
    // The store held this object: let go of it now.
    console = nil
    for source in sources { source.cancel() }
    sources = []
    sink.finish()
  }

  /// Something may have changed.
  @objc private func changed() {
    sink.yield()
  }

  /// Watches the console user's key in the dynamic store, on the main run loop. The store holds this object until it
  /// goes, so its callback never outlives it.
  ///
  /// - Returns: The store and its run loop source, or nil when the store could not be watched.
  private func followConsoleUser() -> (store: SCDynamicStore, source: CFRunLoopSource)? {
    var context = SCDynamicStoreContext(
      version: 0,
      info: Unmanaged.passUnretained(self)
        .toOpaque(),
      retain: { info in
        _ = Unmanaged<SetupChanges>.fromOpaque(info)
          .retain()
        return info
      },
      release: { info in
        Unmanaged<SetupChanges>.fromOpaque(info)
          .release()
      },
      copyDescription: nil
    )
    let store = SCDynamicStoreCreate(
      nil,
      "Atlas setup" as CFString,
      { _, _, info in
        guard let info else { return }
        // On the main run loop, where the source was added; the store holds this object while it can call.
        MainActor.assumeIsolated {
          Unmanaged<SetupChanges>.fromOpaque(info)
            .takeUnretainedValue()
            .changed()
        }
      },
      &context
    )
    guard let store,
      SCDynamicStoreSetNotificationKeys(store, [SCDynamicStoreKeyCreateConsoleUser(nil)] as CFArray, nil),
      let source = SCDynamicStoreCreateRunLoopSource(nil, store, 0)
    else { return nil }
    CFRunLoopAddSource(CFRunLoopGetMain(), source, .commonModes)
    return (store, source)
  }

  /// Watches each folder, or the nearest one above it that can be opened, afresh: after a change the folder may have
  /// come into being, or been replaced. The new watches start before the old ones end, so nothing in between is
  /// missed.
  private func rearm() {
    let old = sources
    sources = folders.compactMap(watching(nearest:))
    for source in old { source.cancel() }
  }

  /// A source for the folder at `folder`, or for the nearest folder above it that exists and opens.
  ///
  /// - Parameter folder: The folder's path.
  /// - Returns: The running source, or nil when not even the root opens.
  private func watching(nearest folder: String) -> (any DispatchSourceFileSystemObject)? {
    var candidate = Self.nearestExisting(folder)
    while let path = candidate {
      if let source = source(watching: path) { return source }
      // Gone between the lookup and the open: the folder above stands in.
      let parent = URL(fileURLWithPath: path)
        .deletingLastPathComponent()
        .path
      candidate = parent == path ? nil : Self.nearestExisting(parent)
    }
    return nil
  }

  /// A source for changes in the folder at `path`, which reports them and watches afresh.
  ///
  /// - Parameter path: The folder.
  /// - Returns: The running source, or nil when the folder could not be opened.
  private func source(watching path: String) -> (any DispatchSourceFileSystemObject)? {
    let descriptor = open(path, O_EVTONLY)
    guard descriptor >= 0 else { return nil }
    let source = DispatchSource.makeFileSystemObjectSource(
      fileDescriptor: descriptor,
      eventMask: [.write, .delete, .rename, .link, .attrib, .extend],
      queue: .main
    )
    source.setEventHandler { [weak self] in
      MainActor.assumeIsolated {
        guard let self, !self.stopped else { return }
        self.changed()
        self.rearm()
      }
    }
    source.setCancelHandler { close(descriptor) }
    source.resume()
    return source
  }

  /// `folder` when it exists, else the nearest folder above it that does.
  ///
  /// - Parameter folder: The folder's path.
  /// - Returns: The path to watch, or nil when not even the root can be found.
  static func nearestExisting(_ folder: String) -> String? {
    var url = URL(fileURLWithPath: folder)
    while true {
      var isFolder: ObjCBool = false
      if FileManager.default.fileExists(atPath: url.path, isDirectory: &isFolder), isFolder.boolValue {
        return url.path
      }
      let parent = url.deletingLastPathComponent()
      guard parent.path != url.path else { return nil }
      url = parent
    }
  }
}
