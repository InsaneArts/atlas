import AtlasVMKit
import Foundation

/// `AtlasVM forget-passwords <desktop> --removal <change>`: the station's
/// way to have a removed VM's passwords deleted when its runner cannot run.
/// It deletes them, keeps what became of each beside the removal's record
/// and exits; it starts no VM and serves nothing.
enum ForgetPasswordsEntry {
  /// Whether `arguments` ask for it.
  static func asked(_ arguments: [String]) -> Bool {
    arguments.count > 1 && arguments[1] == "forget-passwords"
  }

  /// Run it as `arguments` ask, for the VMs in `parent`: its exit status,
  /// zero once what became of each password is kept.
  static func run(_ arguments: [String], parent: URL) -> Int32 {
    guard arguments.count == 5, arguments[3] == "--removal",
      let folder = VMFolder(in: parent, desktop: arguments[2]),
      let removal = ChangeID(arguments[4])
    else {
      FileHandle.standardError
        .write(Data("usage: AtlasVM forget-passwords <desktop> --removal <change>\n".utf8))
      return 64
    }
    do {
      let forgotten = try PasswordForgetting.forget(folder, removal: removal, delete: GuestPassword.delete)
      let left = forgotten.passwords
        .filter { $0.value == .left }
        .keys
        .sorted()
      if !left.isEmpty {
        FileHandle.standardError
          .write(Data("Left, as they could not be deleted without asking: \(left.joined(separator: ", "))\n".utf8))
      }
      return 0
    } catch {
      FileHandle.standardError
        .write(Data("\(error)\n".utf8))
      return 1
    }
  }
}
