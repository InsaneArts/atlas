import Foundation

/// The deletion-only entry: a removed VM's two passwords deleted by Atlas
/// VM, which made them and so alone deletes them without asking, when the
/// VM's runner cannot run to do it. Only under the removal that names it,
/// once that removal came to its passwords; it never starts the VM, nor
/// touches its folder.
public enum PasswordForgetting {
  /// Delete the passwords of `folder`'s VM through `delete`, under the
  /// removal `removal`, and keep what became of each beside the removal's
  /// record before saying so.
  public static func forget(
    _ folder: VMFolder,
    removal: ChangeID,
    delete: (String) -> PasswordFate
  ) throws(PasswordForgettingError) -> ForgottenPasswords {
    let barrier: RemovalBarrier?
    do {
      try VMFolder.checkPrivate(folder.removal.deletingLastPathComponent())
      barrier = try RemovalBarrier.read(from: folder.removal)
    } catch {
      throw .noRemoval
    }
    guard let barrier else { throw .noRemoval }
    guard barrier.change == removal else { throw .anotherRemoval }
    guard barrier.step == RemovalBarrier.passwords else { throw .notYet }
    var passwords: [String: PasswordFate] = [:]
    for account in folder.passwordAccounts {
      passwords[account] = delete(account)
    }
    let forgotten = ForgottenPasswords(change: removal, passwords: passwords)
    do {
      try forgotten.write(to: folder.forgottenPasswords)
    } catch {
      throw .unkept
    }
    return forgotten
  }
}
