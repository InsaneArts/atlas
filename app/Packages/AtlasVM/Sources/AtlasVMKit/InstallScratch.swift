import Darwin
import Foundation

/// The scratch folder Apple's installer leaves behind for each install of
/// macOS in a VM, `com.apple.Virtualization.Installation.*` in this
/// account's temporary folder, which its framework gives no say over.
/// Once an install ends, its own is removed: a folder new since the install
/// began whose log names the very restore image it installed from. A
/// folder there before, or naming another image or none, is another's and
/// is never touched.
public struct InstallScratch: Sendable {
  /// How the installer begins its scratch folder's name.
  static let prefix = "com.apple.Virtualization.Installation."
  /// How much of an installer's log is read for the image it names.
  private static let logRead = 1 << 20

  private let folder: URL
  private let image: URL
  private let before: Set<String>

  /// The scratch an install from `image` is about to leave in `folder`:
  /// what is there now is another's.
  public init(in folder: URL, image: URL) {
    self.folder = folder
    self.image = image
    before = Set(Self.names(in: folder))
  }

  /// This account's temporary folder, where the installer keeps its
  /// scratch; none when macOS does not say.
  public static var temporaryFolder: URL? {
    let length = confstr(_CS_DARWIN_USER_TEMP_DIR, nil, 0)
    guard length > 0 else { return nil }
    var path = [CChar](repeating: 0, count: length)
    guard confstr(_CS_DARWIN_USER_TEMP_DIR, &path, length) == length else { return nil }
    let bytes = path.prefix { $0 != 0 }
      .map { UInt8(bitPattern: $0) }
    return URL(filePath: String(decoding: bytes, as: UTF8.self), directoryHint: .isDirectory)
  }

  /// Run `install` from `image`, then remove the scratch it left in
  /// `folder`, whether it was done, failed or cancelled. With no folder
  /// known, nothing is removed.
  public static func cleaning<T>(
    in folder: URL?,
    image: URL,
    _ install: () async throws -> T
  ) async throws -> T {
    let scratch = folder.map { InstallScratch(in: $0, image: image) }
    defer { _ = scratch?.remove() }
    return try await install()
  }

  /// Remove the install's own scratch: the folders it found removed.
  @discardableResult
  public func remove() -> [URL] {
    Self.names(in: folder)
      .filter { !before.contains($0) }
      .map { folder.appending(path: $0, directoryHint: .notDirectory) }
      .filter(isOwn)
      .filter { (try? FileManager.default.removeItem(at: $0)) != nil }
  }

  /// The names in `folder` the installer's scratch folders begin with.
  private static func names(in folder: URL) -> [String] {
    let names = (try? FileManager.default.contentsOfDirectory(atPath: folder.path(percentEncoded: false))) ?? []
    return names.filter { $0.hasPrefix(prefix) }
  }

  /// Whether `scratch` is a folder of this account's, never a link, whose
  /// log names this install's image.
  private func isOwn(_ scratch: URL) -> Bool {
    var status = stat()
    guard lstat(scratch.path(percentEncoded: false), &status) == 0, status.st_mode & S_IFMT == S_IFDIR,
      status.st_uid == getuid()
    else {
      return false
    }
    let logs = (try? FileManager.default.contentsOfDirectory(atPath: scratch.path(percentEncoded: false))) ?? []
    let named = "Restore Bundle Path: \(image.path(percentEncoded: false))"
    return
      logs
      .filter { $0.hasPrefix("Logs.") }
      .contains { name in
        let log = scratch.appending(path: name, directoryHint: .isDirectory)
          .appending(path: "log_host.txt")
        return Self.head(of: log)?
          .split(whereSeparator: \.isNewline)
          .contains { $0.hasSuffix(named) } == true
      }
  }

  /// The beginning of the regular file `file`, never through a link.
  private static func head(of file: URL) -> String? {
    let descriptor = open(file.path(percentEncoded: false), O_RDONLY | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC)
    guard descriptor >= 0 else { return nil }
    let handle = FileHandle(fileDescriptor: descriptor, closeOnDealloc: true)
    var status = stat()
    guard fstat(descriptor, &status) == 0, status.st_mode & S_IFMT == S_IFREG,
      let data = try? handle.read(upToCount: logRead)
    else {
      return nil
    }
    return String(decoding: data, as: UTF8.self)
  }
}
