import Darwin
import Foundation

/// The changes a station asked a VM's runner for, each kept as asked before
/// anything is done and as settled once it is, in a private folder: a
/// change asked again is answered from its record and never done twice,
/// and one a runner was doing when it went is found again.
public struct ChangeLedger: Sendable {
  /// The folder its records are in.
  public let folder: URL

  /// The longest record read.
  private static let longest = 64 * 1024

  /// The records in `folder`, a private folder of this account's.
  public init(folder: URL) {
    self.folder = folder
  }

  /// Ask for `operation`, a change, by the run `run`: kept as intended when
  /// it is new, before it is done.
  public func begin(_ operation: ControlOperation, by run: String) throws -> ChangeBeginning {
    guard let change = operation.change else { return .conflicting }
    if let kept = try record(change) {
      return kept.operation == operation ? .asked(kept) : .conflicting
    }
    try keep(ChangeRecord(operation: operation, run: run), for: change)
    return .new
  }

  /// Keep that `record`, begun before, came to `outcome`.
  public func settle(_ record: ChangeRecord, as outcome: ChangeOutcome) throws {
    guard let change = record.operation.change else { return }
    var settled = record
    settled.outcome = outcome
    try keep(settled, for: change)
  }

  /// The record of the change `change`, as it is kept for good; none when it
  /// was never asked.
  public func record(_ change: ChangeID) throws -> ChangeRecord? {
    try VMFolder.checkPrivate(folder)
    let file = folder.appending(path: Self.name(of: change))
    guard let data = try Self.read(file) else { return nil }
    guard let record = try? JSONDecoder().decode(ChangeRecord.self, from: data),
      record.operation.change == change
    else {
      throw ChangeLedgerError.unreadable(file)
    }
    // A record seen is answered only once its folder keeps it.
    try sync(for: file)
    return record
  }

  /// Every change kept as intended and not yet settled.
  public func unsettled() throws -> [ChangeRecord] {
    try VMFolder.checkPrivate(folder)
    let names: [String]
    do {
      names = try FileManager.default.contentsOfDirectory(atPath: folder.path(percentEncoded: false))
    } catch {
      throw ChangeLedgerError.unreadable(folder)
    }
    return try names.sorted().compactMap { name in
      guard name.hasSuffix(".json"), let change = ChangeID(String(name.dropLast(5))) else {
        return nil
      }
      return try record(change).flatMap { $0.outcome == nil ? $0 : nil }
    }
  }

  private static func name(of change: ChangeID) -> String {
    "\(change.text).json"
  }

  /// The bytes of the record `file`: none when there is none; never through
  /// a link, and only from a file of this account's that others may not
  /// change.
  private static func read(_ file: URL) throws -> Data? {
    // Non-blocking, so a pipe in a record's place never waits for a writer.
    let descriptor = open(file.path(percentEncoded: false), O_RDONLY | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC)
    guard descriptor >= 0 else {
      if errno == ENOENT { return nil }
      throw ChangeLedgerError.unreadable(file)
    }
    let handle = FileHandle(fileDescriptor: descriptor, closeOnDealloc: true)
    var status = stat()
    guard fstat(descriptor, &status) == 0, status.st_mode & S_IFMT == S_IFREG,
      status.st_uid == getuid(), status.st_mode & 0o022 == 0,
      let data = try? handle.read(upToCount: longest + 1), data.count <= longest
    else {
      throw ChangeLedgerError.unreadable(file)
    }
    return data
  }

  /// Write `record` whole, then put it in place by one rename, each made to
  /// outlive a crash.
  private func keep(_ record: ChangeRecord, for change: ChangeID) throws {
    try VMFolder.checkPrivate(folder)
    let target = folder.appending(path: Self.name(of: change))
    // Never one longer than is read back.
    guard let data = try? JSONEncoder().encode(record), data.count <= Self.longest else {
      throw ChangeLedgerError.unwritable(target)
    }
    let writing = folder.appending(path: ".\(Self.name(of: change)).writing").path(percentEncoded: false)
    unlink(writing)
    let descriptor = open(writing, O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC, 0o600)
    guard descriptor >= 0 else { throw ChangeLedgerError.unwritable(target) }
    let written = data.withUnsafeBytes { bytes in Darwin.write(descriptor, bytes.baseAddress, bytes.count) }
    let synced = Self.durable(descriptor)
    close(descriptor)
    guard written == data.count, synced, rename(writing, target.path(percentEncoded: false)) == 0 else {
      unlink(writing)
      throw ChangeLedgerError.unwritable(target)
    }
    try sync(for: target)
  }

  /// Make what changed in the folder outlive a crash, for `file`'s sake.
  private func sync(for file: URL) throws {
    let descriptor = open(folder.path(percentEncoded: false), O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
    guard descriptor >= 0 else { throw ChangeLedgerError.unwritable(file) }
    let kept = Self.durable(descriptor)
    close(descriptor)
    guard kept else { throw ChangeLedgerError.unwritable(file) }
  }

  /// Make what was written through `descriptor` reach the disk itself, past
  /// its cache, as `fsync` alone does not on macOS; `fsync` where the disk
  /// cannot.
  private static func durable(_ descriptor: Int32) -> Bool {
    fcntl(descriptor, F_FULLFSYNC) == 0 || fsync(descriptor) == 0
  }
}
