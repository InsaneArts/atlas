/// One section of the sidebar's sessions and the rows it draws under its heading.
public struct ViewerSessionGroup: Equatable, Sendable, Identifiable {
  /// What its sessions do.
  public var section: ViewerSessionSection
  /// The rows it draws, in order; Ended's are only the chosen one's while it is folded.
  public var rows: [ViewerSessionRow]

  /// The group's identity: its section.
  public var id: ViewerSessionSection { section }

  /// Creates a group.
  ///
  /// - Parameters:
  ///   - section: What its sessions do.
  ///   - rows: The rows it draws.
  public init(_ section: ViewerSessionSection, rows: [ViewerSessionRow]) {
    self.section = section
    self.rows = rows
  }
}
