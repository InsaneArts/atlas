import Darwin
import Foundation

/// Where everything of one VM is kept: its disk and identity, the folder it
/// shares with its guest, its changes, its runs and its sockets. Private to
/// the person's account, and checked to be on every use.
public struct VMFolder: Equatable, Sendable {
  /// The folder itself.
  public let root: URL
  /// The folder every VM's folder is in.
  public let parent: URL
  /// The desktop it is the VM of.
  private let desktop: String

  /// The folder of the desktop `desktop` in `parent`; none unless `desktop`
  /// is a desktop's name, 24 lowercase hexadecimal digits.
  public init?(in parent: URL, desktop: String) {
    let digits = desktop.utf8.allSatisfy { byte in
      (UInt8(ascii: "0")...UInt8(ascii: "9"))
        .contains(byte)
        || (UInt8(ascii: "a")...UInt8(ascii: "f"))
          .contains(byte)
    }
    guard desktop.utf8.count == 24, digits else { return nil }
    root = parent.appending(path: desktop, directoryHint: .isDirectory)
    self.parent = parent
    self.desktop = desktop
  }

  /// The lock every run of it holds for its whole life, and its removal
  /// from its proof to its end: outside the folder a removal deletes.
  public var lifecycleLock: URL {
    parent.appending(path: ".locks/\(desktop).lock")
  }

  /// The record of its removal, kept outside its folder: while it is there,
  /// no run of it starts.
  public var removal: URL {
    parent.appending(path: ".removals/\(desktop).json")
  }

  /// Where every VM's folder is, in the home folder `home`.
  public static func parent(home: URL) -> URL {
    home.appending(path: "Library/Application Support/Atlas/VMs", directoryHint: .isDirectory)
  }

  /// Its disk.
  public var disk: URL { root.appending(path: "Disk.asif") }
  /// Its auxiliary storage, which its boot needs.
  public var auxiliaryStorage: URL { root.appending(path: "AuxiliaryStorage") }
  /// The hardware model it was installed for.
  public var hardwareModel: URL { root.appending(path: "HardwareModel") }
  /// Its machine's identifier.
  public var machineIdentifier: URL { root.appending(path: "MachineIdentifier") }
  /// The folder its guest sees, read-only.
  public var share: URL { root.appending(path: "Share", directoryHint: .isDirectory) }
  /// The changes its station asked for.
  public var changes: URL { root.appending(path: "Changes", directoryHint: .isDirectory) }
  /// The lock the run serving it holds.
  public var lock: URL { root.appending(path: "Runner.lock") }
  /// The run of its runner it was last served by.
  public var incarnation: URL { root.appending(path: "Incarnation.json") }
  /// The socket its station controls it on.
  public var control: URL { root.appending(path: "control") }
  /// The socket its station's session reaches its guest on.
  public var station: URL { root.appending(path: "station") }

  /// Make it and its folders if they are not there, each private, and
  /// check that each is.
  public func prepare() throws {
    for folder in [root, share, changes] {
      try Self.makePrivate(folder)
    }
  }

  /// Make `folder` if it is not there, mode 0700, and check it is a folder
  /// of this account's that no one else may read or change.
  public static func makePrivate(_ folder: URL) throws {
    do {
      try FileManager.default
        .createDirectory(
          at: folder,
          withIntermediateDirectories: true,
          attributes: [.posixPermissions: 0o700]
        )
    } catch {
      throw VMFolderError.unreadable(folder)
    }
    try checkPrivate(folder)
  }

  /// Check `folder` is a folder of this account's, never a link, that no one
  /// else may read or change.
  public static func checkPrivate(_ folder: URL) throws {
    var status = stat()
    guard lstat(folder.path(percentEncoded: false), &status) == 0 else {
      throw VMFolderError.unreadable(folder)
    }
    guard status.st_mode & S_IFMT == S_IFDIR else { throw VMFolderError.notAFolder(folder) }
    guard status.st_uid == getuid(), status.st_mode & 0o077 == 0,
      !grantsAccess(folder.path(percentEncoded: false))
    else {
      throw VMFolderError.notPrivate(folder)
    }
  }

  /// Whether `path` carries an access entry that lets anyone in, which its
  /// mode does not show; one that cannot be read counts as one.
  private static func grantsAccess(_ path: String) -> Bool {
    guard let entries = acl_get_link_np(path, ACL_TYPE_EXTENDED) else { return errno != ENOENT }
    defer { acl_free(UnsafeMutableRawPointer(entries)) }
    var entry: acl_entry_t?
    var which = ACL_FIRST_ENTRY.rawValue
    while acl_get_entry(entries, which, &entry) == 0, let current = entry {
      var tag = ACL_UNDEFINED_TAG
      if acl_get_tag_type(current, &tag) != 0 || tag == ACL_EXTENDED_ALLOW {
        return true
      }
      which = ACL_NEXT_ENTRY.rawValue
    }
    return false
  }
}
