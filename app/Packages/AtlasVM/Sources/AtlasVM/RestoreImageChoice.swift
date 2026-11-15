import Foundation

/// The restore image a VM is made from: where it is kept, and where to
/// download it from when it is not kept yet.
struct RestoreImageChoice {
  /// Where it is, or will be once downloaded.
  let file: URL
  /// Where it is downloaded from; none when it is kept already.
  let source: URL?
}
