import Darwin
import Foundation

/// A record file in a private folder: read only from a regular file of this
/// account's that no one else may change, never through a link; written
/// whole, put in place by one rename, and made to reach the disk itself.
public enum PrivateFile {
  /// The bytes of `file`, at most `limit` of them; none when there is none.
  public static func read(_ file: URL, limit: Int) throws(PrivateFileError) -> Data? {
    // Non-blocking, so a pipe in a record's place never waits for a writer.
    let descriptor = open(file.path(percentEncoded: false), O_RDONLY | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC)
    guard descriptor >= 0 else {
      if errno == ENOENT { return nil }
      throw .unreadable(file)
    }
    let handle = FileHandle(fileDescriptor: descriptor, closeOnDealloc: true)
    var status = stat()
    guard fstat(descriptor, &status) == 0, status.st_mode & S_IFMT == S_IFREG,
      status.st_uid == getuid(), status.st_mode & 0o022 == 0,
      let data = try? handle.read(upToCount: limit + 1) ?? Data(), data.count <= limit
    else {
      throw .unreadable(file)
    }
    return data
  }

  /// Put `data` at `file`, whole, mode 0600, so it outlives a crash or the
  /// power going.
  public static func write(_ data: Data, to file: URL) throws(PrivateFileError) {
    let folder = file.deletingLastPathComponent()
    let writing = folder.appending(path: ".\(file.lastPathComponent).writing")
      .path(percentEncoded: false)
    unlink(writing)
    let descriptor = open(writing, O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC, 0o600)
    guard descriptor >= 0 else { throw .unwritable(file) }
    let written = data.withUnsafeBytes { bytes in Darwin.write(descriptor, bytes.baseAddress, bytes.count) }
    let synced = durable(descriptor)
    close(descriptor)
    guard written == data.count, synced, rename(writing, file.path(percentEncoded: false)) == 0 else {
      unlink(writing)
      throw .unwritable(file)
    }
    try sync(folder, for: file)
  }

  /// Remove `file`, unless it is gone already, and make its going reach
  /// the disk.
  public static func remove(_ file: URL) throws(PrivateFileError) {
    guard unlink(file.path(percentEncoded: false)) == 0 || errno == ENOENT else { throw .unwritable(file) }
    try sync(file.deletingLastPathComponent(), for: file)
  }

  /// Make what changed in `folder` reach the disk, for `file`'s sake.
  public static func sync(_ folder: URL, for file: URL) throws(PrivateFileError) {
    let descriptor = open(folder.path(percentEncoded: false), O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
    guard descriptor >= 0 else { throw .unwritable(file) }
    let kept = durable(descriptor)
    close(descriptor)
    guard kept else { throw .unwritable(file) }
  }

  /// Make what was written through `descriptor` reach the disk itself, past
  /// its cache, as `fsync` alone does not on macOS; `fsync` where the disk
  /// cannot.
  private static func durable(_ descriptor: Int32) -> Bool {
    fcntl(descriptor, F_FULLFSYNC) == 0 || fsync(descriptor) == 0
  }
}
