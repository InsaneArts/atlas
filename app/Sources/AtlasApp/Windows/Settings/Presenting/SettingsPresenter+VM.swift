import AtlasService
import AtlasStatus
import AtlasUI

extension SettingsPresenter {
  /// The page of the VM shown: one opened from Desktops, or the one desktop on the page in Helper's place. Its card
  /// with its dot and screen's size, then the rows AtlasStatus decides it can show, in their sections; each button
  /// waits while one of its commands or an alert runs. Nil while no VM's page shows.
  ///
  /// - Parameter snapshot: What the controller holds and reads.
  /// - Returns: The page, or nil.
  static func vmPage(_ snapshot: SettingsSnapshot) -> SettingsVMState? {
    guard let listed = snapshot.desktops?.desktops else { return nil }
    let shown = route(snapshot)
    let entry: DesktopList.Entry? =
      switch shown {
      case .desktop(let id): listed.first { $0.id == id && $0.kind == .vm }
      case .page(.helper) where listed.count == 1: listed.first { $0.kind == .vm }
      default: nil
      }
    guard let entry else { return nil }
    let session = snapshot.session
    let working = session.desktopWorking == entry.id
    let idle = !working && !session.alertRunning
    let decided = VMPageRows.of(entry)
    let sections = VMPageSection.allCases
      .compactMap { section -> SettingsVMSection? in
        let rows =
          decided
          .filter { $0.section == section }
          .map { row(of: $0, vm: entry.id, idle: idle) }
        guard !rows.isEmpty else { return nil }
        return SettingsVMSection(title: title(of: section), rows: rows)
      }
    return SettingsVMState(
      id: entry.id,
      title: Words.title(of: entry),
      line: SettingsWords.vmLine(display: Words.size(entry.facts?.display)),
      tone: tone(DesktopStanding.of(entry)),
      sections: sections,
      waiting: working ? session.desktopWaiting : nil,
      notice: notice(snapshot, on: shown, busy: working),
      addsDesktop: listed.count == 1
    )
  }

  /// A section's heading, as every Settings page has them; none over Remove, as an account's page has none.
  ///
  /// - Parameter section: The section.
  /// - Returns: Its heading, or nil.
  static func title(of section: VMPageSection) -> String? {
    switch section {
    case .use: SettingsWords.useSection
    case .about: SettingsWords.aboutSection
    case .removal: nil
    }
  }

  /// One of a VM page's rows, worded, its button sending what it does for the VM `vm`.
  ///
  /// - Parameters:
  ///   - row: The row AtlasStatus decided.
  ///   - vm: The VM's ID.
  ///   - idle: Whether nothing on its page runs now, so its buttons can.
  /// - Returns: The row.
  static func row(of row: VMPageRow, vm: String, idle: Bool) -> SettingsVMRow {
    switch row {
    case .power(let power, let button):
      let shown: SettingsVMButton? =
        switch button {
        case .stop(let enabled):
          SettingsVMButton(title: SettingsWords.stop, event: .askStopVM(vm), isEnabled: enabled && idle)
        case .start(let enabled):
          SettingsVMButton(title: SettingsWords.start, event: .startVM(vm), isEnabled: enabled && idle)
        case nil: nil
        }
      return SettingsVMRow(
        id: "power",
        title: SettingsWords.power(power),
        detail: SettingsWords.startsByItself,
        button: shown
      )
    case .maintenance(let isOn, let enabled):
      return SettingsVMRow(
        id: "maintenance",
        title: SettingsWords.maintenanceTitle,
        detail: isOn ? SettingsWords.maintenanceOn : SettingsWords.maintenanceDetail,
        button: SettingsVMButton(
          title: isOn ? SettingsWords.endMaintenance : SettingsWords.openMaintenance,
          event: isOn ? .endMaintenance(vm) : .maintainVM(vm),
          isEnabled: enabled && idle
        )
      )
    case .disk(let used, let size):
      return SettingsVMRow(id: "disk", title: SettingsWords.disk, value: SettingsWords.diskUse(used: used, size: size))
    case .macOS(let version, let newer):
      return SettingsVMRow(
        id: "macos",
        title: SettingsWords.macOS(version),
        detail: newer.map(SettingsWords.newerMacOS)
      )
    case .atlas(let state, let next):
      return SettingsVMRow(
        id: "atlas",
        title: SettingsWords.atlasInVM,
        detail: state == .available || state == .newer ? next : nil,
        value: SettingsWords.atlas(state)
      )
    case .remove:
      return SettingsVMRow(
        id: "remove",
        title: SettingsWords.removeVMTitle,
        detail: SettingsWords.removeVMDetail,
        button: SettingsVMButton(
          title: SettingsWords.removeVM,
          event: .removeVM(vm),
          isDestructive: true,
          isEnabled: idle
        )
      )
    }
  }
}
