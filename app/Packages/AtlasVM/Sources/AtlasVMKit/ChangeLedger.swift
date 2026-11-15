import Foundation

/// The changes a station asked a VM's runner for, each kept as asked before
/// anything is done and as settled once it is, in a private folder: a
/// change asked again is answered from its record and never done twice,
/// and one a runner was doing when it went is found again.
public struct ChangeLedger: Sendable {
  /// The folder its records are in.
  public let folder: URL

  /// The longest record read or written.
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
    let data: Data?
    do {
      data = try PrivateFile.read(file, limit: Self.longest)
      // A record seen is answered only once its folder keeps it.
      if data != nil {
        try PrivateFile.sync(folder, for: file)
      }
    } catch {
      throw ChangeLedgerError.unreadable(file)
    }
    guard let data else { return nil }
    guard let record = try? JSONDecoder().decode(ChangeRecord.self, from: data),
      record.operation.change == change
    else {
      throw ChangeLedgerError.unreadable(file)
    }
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

  private func keep(_ record: ChangeRecord, for change: ChangeID) throws {
    try VMFolder.checkPrivate(folder)
    let target = folder.appending(path: Self.name(of: change))
    // Never one longer than is read back.
    guard let data = try? JSONEncoder().encode(record), data.count <= Self.longest else {
      throw ChangeLedgerError.unwritable(target)
    }
    do {
      try PrivateFile.write(data, to: target)
    } catch {
      throw ChangeLedgerError.unwritable(target)
    }
  }
}
