import Darwin
import Foundation
import Testing

@testable import AtlasVMKit

@Suite("A removed VM's passwords deleted when its runner cannot run")
struct PasswordForgettingTests {
  private let desktop = String(repeating: "ab", count: 12)

  /// A VMs folder of its own, the VM in it named `desktop`, and its
  /// removal recorded under `change` at `step` when one is given.
  private func removed(_ folders: Folders, change: ChangeID?, step: String = "passwords") throws -> VMFolder {
    let parent = try folders.folder("VMs")
    let folder = try #require(VMFolder(in: parent, desktop: desktop))
    try VMFolder.makePrivate(folder.removal.deletingLastPathComponent())
    if let change {
      let record = #"{"change":"\#(change.text)","step":"\#(step)","folder":null,"passwords":{},"kept":[]}"#
      try PrivateFile.write(Data(record.utf8), to: folder.removal)
    }
    return folder
  }

  @Test("it is refused without its removal's record, deleting nothing and keeping nothing")
  func refusedWithoutTheBarrier() throws {
    let folders = try Folders()
    let folder = try removed(folders, change: nil)
    var asked: [String] = []

    #expect(throws: PasswordForgettingError.noRemoval) {
      try PasswordForgetting.forget(folder, removal: .repeating("a0")) { account in
        asked.append(account)
        return .deleted
      }
    }
    #expect(asked.isEmpty)
    #expect(!FileManager.default.fileExists(atPath: folder.forgottenPasswords.path(percentEncoded: false)))
  }

  @Test("it is refused under another removal, and before its removal came to the passwords")
  func refusedUnlessTheBarrierNamesThisRemoval() throws {
    let other = try Folders()
    let early = try Folders()
    let another = try removed(other, change: .repeating("b0"))
    let proving = try removed(early, change: .repeating("a0"), step: "proving")
    var asked: [String] = []

    #expect(throws: PasswordForgettingError.anotherRemoval) {
      try PasswordForgetting.forget(another, removal: .repeating("a0")) { account in
        asked.append(account)
        return .deleted
      }
    }
    #expect(throws: PasswordForgettingError.notYet) {
      try PasswordForgetting.forget(proving, removal: .repeating("a0")) { account in
        asked.append(account)
        return .deleted
      }
    }
    #expect(asked.isEmpty)
  }

  @Test("it never starts the VM: it runs while the station holds the VM's lock and makes nothing of the VM")
  func neverStartsTheVM() throws {
    let folders = try Folders()
    let folder = try removed(folders, change: .repeating("a0"))
    try VMFolder.makePrivate(folder.lifecycleLock.deletingLastPathComponent())
    let station = try #require(FileLock(at: folder.lifecycleLock))
    var asked: [String] = []

    let forgotten = try PasswordForgetting.forget(folder, removal: .repeating("a0")) { account in
      asked.append(account)
      return .deleted
    }

    #expect(asked == [desktop, "\(desktop).admin"])
    #expect(forgotten.passwords.count == 2)
    #expect(!FileManager.default.fileExists(atPath: folder.root.path(percentEncoded: false)))
    #expect(FileLock(at: folder.lifecycleLock) == nil, "the station's hold is never taken")
    withExtendedLifetime(station) {}
  }

  @Test("what became of each password is kept beside the removal's record, private, and read back as it was")
  func itsResultIsReadBack() throws {
    let folders = try Folders()
    let folder = try removed(folders, change: .repeating("a0"))
    let fates: [String: PasswordFate] = [desktop: .deleted, "\(desktop).admin": .left]

    let forgotten = try PasswordForgetting.forget(folder, removal: .repeating("a0")) { fates[$0] ?? .absent }
    let kept = try ForgottenPasswords.read(from: folder.forgottenPasswords)
    var status = stat()
    let looked = lstat(folder.forgottenPasswords.path(percentEncoded: false), &status)

    #expect(kept == forgotten)
    #expect(kept == ForgottenPasswords(change: .repeating("a0"), passwords: fates))
    #expect(looked == 0 && status.st_mode & 0o777 == 0o600)
    #expect(folder.forgottenPasswords.deletingLastPathComponent() == folder.removal.deletingLastPathComponent())
  }

  @Test("its result is written as the station reads it")
  func itsResultIsTheStationsForm() throws {
    let forgotten = ForgottenPasswords(change: .repeating("a0"), passwords: [desktop: .absent])
    let encoder = JSONEncoder()
    encoder.outputFormatting = .sortedKeys

    let text = String(decoding: try encoder.encode(forgotten), as: UTF8.self)

    #expect(text == #"{"change":"\#(String(repeating: "a0", count: 24))","passwords":{"\#(desktop)":"absent"}}"#)
  }
}
