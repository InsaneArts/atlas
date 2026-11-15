import Foundation

/// Why a VM's folder cannot be used.
public enum VMFolderError: Error, Equatable {
  /// It is not a folder of its own: a link, or a file.
  case notAFolder(URL)
  /// Another account owns it, or others may read or change it.
  case notPrivate(URL)
  /// It could not be made or looked at.
  case unreadable(URL)
}
