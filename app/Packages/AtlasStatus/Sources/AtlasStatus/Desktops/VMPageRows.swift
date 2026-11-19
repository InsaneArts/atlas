import AtlasService

/// Which rows a VM's page shows, from what the service says the person can do with it (`can`) and what its newest
/// status read said: only the rows it can show now, in the page's order. Its power while it is said, the button as
/// `can` allows, Stop… kept but waiting while maintenance holds it; maintenance while it can be opened or ended, or is
/// on; its disk while its size is said; its macOS while its version is; its Atlas while its standing is; Remove while
/// it can be removed.
public enum VMPageRows {
  /// The rows of `entry`'s page.
  ///
  /// - Parameter entry: A VM the service serves.
  /// - Returns: Its rows, in order.
  public static func of(_ entry: DesktopList.Entry) -> [VMPageRow] {
    let facts = entry.facts ?? DesktopList.Facts()
    let can = Set(entry.can)
    let maintained = facts.maintenance == "on" || facts.maintenance == "ending"
    var rows: [VMPageRow] = []
    if let power = VMPower(word: facts.power) {
      rows.append(.power(power, button: button(power, can: can)))
    }
    if can.contains("maintain") || can.contains("end") || maintained {
      rows.append(.maintenance(isOn: maintained, enabled: can.contains(maintained ? "end" : "maintain")))
    }
    if let size = facts.diskSize, size > 0 {
      rows.append(.disk(used: facts.diskUsed, size: size))
    }
    if let version = facts.macOS {
      rows.append(.macOS(version: version, newer: facts.newerMacOS == true ? facts.latestMacOS : nil))
    }
    if facts.atlas != nil {
      rows.append(.atlas(VMPageRow.AtlasState(word: facts.atlas), next: facts.atlasNext))
    }
    if can.contains("remove") { rows.append(.remove) }
    return rows
  }

  /// The power row's button: Stop… while it runs or starts, Start while it is stopped, none while it stops.
  ///
  /// - Parameters:
  ///   - power: Its power.
  ///   - can: What the person can do with it now.
  /// - Returns: The button, or nil.
  private static func button(_ power: VMPower, can: Set<String>) -> VMPageRow.PowerButton? {
    switch power {
    case .running, .starting: .stop(enabled: can.contains("stop"))
    case .stopped: .start(enabled: can.contains("start"))
    case .stopping: nil
    }
  }
}
