import Darwin
import Foundation

/// How much of a VM's disk its guest filled: the bytes its file takes on
/// this Mac's disk, read from the blocks allocated to it, said apart from
/// the size the VM's disk was made with.
public struct DiskUse: Codable, Equatable, Sendable {
  /// The bytes the disk's file takes; none while it cannot be read.
  public var used: UInt64?
  /// The size of the VM's disk, in bytes.
  public var size: UInt64

  /// `used` bytes taken of a disk of `size`.
  public init(used: UInt64?, size: UInt64) {
    self.used = used
    self.size = size
  }

  /// The use of the disk file `file`, of a VM whose disk is `size` bytes:
  /// what it takes none while it is not a regular file that can be read.
  public static func of(_ file: URL, size: UInt64) -> DiskUse {
    var status = stat()
    guard lstat(file.path(percentEncoded: false), &status) == 0, status.st_mode & S_IFMT == S_IFREG,
      status.st_blocks >= 0
    else {
      return DiskUse(used: nil, size: size)
    }
    // Counted in 512-byte blocks, whatever the file system's own block.
    return DiskUse(used: UInt64(status.st_blocks) * UInt64(S_BLKSIZE), size: size)
  }
}
