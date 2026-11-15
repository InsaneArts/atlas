import Darwin
import Foundation

/// The file this program was started from, as the kernel mapped it, and
/// the path it was launched by: replaced once that path names another
/// file, or none, as an update of Atlas leaves it.
struct LaunchedProgram {
  /// The launch path.
  private let path: String
  /// The device of the file its image was mapped from.
  private let device: UInt32
  /// That file's inode.
  private let inode: UInt64

  /// This program's; none when the kernel does not say which file it runs.
  init?() {
    guard let path = Bundle.main.executableURL?.path(percentEncoded: false) else { return nil }
    // The image itself, never what its path names now: an update may have
    // replaced the file there before this run looked.
    var region = proc_regionwithpathinfo()
    let size = Int32(MemoryLayout<proc_regionwithpathinfo>.size)
    let image = UInt64(UInt(bitPattern: #dsohandle))
    guard proc_pidinfo(getpid(), PROC_PIDREGIONPATHINFO, image, &region, size) == size else { return nil }
    // The kernel may answer whole without naming the file: no inode, or no
    // path, names none.
    let named = withUnsafeBytes(of: region.prp_vip.vip_path) { $0.first ?? 0 } != 0
    guard named, region.prp_vip.vip_vi.vi_stat.vst_ino != 0 else { return nil }
    self.path = path
    device = region.prp_vip.vip_vi.vi_stat.vst_dev
    inode = region.prp_vip.vip_vi.vi_stat.vst_ino
  }

  /// Whether the launch path now names another file, or none. A read that
  /// fails otherwise is no replacement.
  var replaced: Bool {
    var status = stat()
    guard stat(path, &status) == 0 else { return errno == ENOENT }
    return UInt32(bitPattern: status.st_dev) != device || status.st_ino != inode
  }
}
