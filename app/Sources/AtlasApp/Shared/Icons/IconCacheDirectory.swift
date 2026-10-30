import Darwin
import Foundation

/// An owned directory descriptor. Every ancestor is opened without following links; subsequent cache operations
/// use only single names relative to this descriptor, including after an ancestor is renamed or replaced.
nonisolated final class IconCacheDirectory: Sendable {
  private let descriptor: Int32

  private init(_ descriptor: Int32) {
    self.descriptor = descriptor
  }

  deinit {
    Darwin.close(descriptor)
  }

  /// Walks from the filesystem root, verifying each component and making missing directories through their parents.
  ///
  /// - Parameters:
  ///   - caches: The absolute caches path, without links in any ancestor.
  ///   - appending: The store's directory components.
  /// - Returns: An owned handle, or nil when a component cannot be verified.
  static func open(_ caches: URL, appending: [String]) -> IconCacheDirectory? {
    guard caches.isFileURL, caches.path.hasPrefix("/") else { return nil }
    let flags = O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC
    var parent = Darwin.open("/", flags)
    guard parent >= 0 else { return nil }
    defer { Darwin.close(parent) }
    for name in caches.pathComponents.dropFirst() + appending {
      guard accepts(name) else { return nil }
      var child = openat(parent, name, flags)
      if child < 0, errno == ENOENT {
        guard mkdirat(parent, name, mode_t(AppIconStore.folderMode)) == 0 || errno == EEXIST else { return nil }
        child = openat(parent, name, flags)
      }
      guard child >= 0 else { return nil }
      Darwin.close(parent)
      parent = child
    }
    let owned = fcntl(parent, F_DUPFD_CLOEXEC, 0)
    return owned >= 0 ? IconCacheDirectory(owned) : nil
  }

  /// Direct child names, enumerated through a fresh stream rather than a path or a shared directory offset.
  var names: [String] {
    let listing = openat(descriptor, ".", O_RDONLY | O_DIRECTORY | O_CLOEXEC)
    guard listing >= 0 else { return [] }
    guard let stream = fdopendir(listing) else {
      Darwin.close(listing)
      return []
    }
    defer { closedir(stream) }
    var names: [String] = []
    while let entry = readdir(stream) {
      let name = withUnsafePointer(to: &entry.pointee.d_name) { pointer in
        pointer.withMemoryRebound(to: CChar.self, capacity: Int(entry.pointee.d_namlen) + 1) {
          String(cString: $0)
        }
      }
      if Self.accepts(name) { names.append(name) }
    }
    return names
  }

  /// Reads a bounded regular file through this directory, refusing links, directories and oversized files.
  ///
  /// - Parameters:
  ///   - name: One child name.
  ///   - largest: The byte limit, also enforced if the file grows during the read.
  /// - Returns: The file's bytes, or nil when refused.
  func read(_ name: String, largest: Int) -> Data? {
    guard Self.accepts(name) else { return nil }
    let file = openat(descriptor, name, O_RDONLY | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC)
    guard file >= 0 else { return nil }
    let handle = FileHandle(fileDescriptor: file, closeOnDealloc: true)
    var status = stat()
    guard fstat(file, &status) == 0, status.st_mode & S_IFMT == S_IFREG,
      status.st_size >= 0, status.st_size <= largest,
      let data = try? handle.read(upToCount: largest + 1), data.count <= largest
    else { return nil }
    return data
  }

  /// Atomically replaces one child using a newly created private file and a descriptor-relative rename.
  ///
  /// - Parameters:
  ///   - data: The complete contents.
  ///   - name: One child name.
  /// - Returns: Whether the replacement succeeded.
  @discardableResult func write(_ data: Data, name: String) -> Bool {
    guard Self.accepts(name) else { return false }
    let part = AppIconStore.partPrefix + UUID().uuidString
    let file = openat(
      descriptor,
      part,
      O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC,
      mode_t(AppIconStore.fileMode)
    )
    guard file >= 0 else { return false }
    let handle = FileHandle(fileDescriptor: file, closeOnDealloc: true)
    defer { remove(part) }
    do {
      try handle.write(contentsOf: data)
      try handle.close()
    } catch {
      return false
    }
    return renameat(descriptor, part, descriptor, name) == 0
  }

  /// Unlinks one direct child, never following a link or recursively removing a directory.
  ///
  /// - Parameter name: One child name.
  func remove(_ name: String) {
    guard Self.accepts(name) else { return }
    unlinkat(descriptor, name, 0)
  }

  private static func accepts(_ name: String) -> Bool {
    !name.isEmpty && name != "." && name != ".." && !name.contains("/") && !name.utf8.contains(0)
  }
}
