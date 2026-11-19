import AtlasService
import Testing

@testable import AtlasStatus

@Suite("Desktop rules")
struct DesktopRulesTests {
  @Test("Second Login is named by its kind, vm and vm-2 by their place, and any other VM by its name")
  func titles() {
    #expect(DesktopTitle.of(name: "main", kind: .user) == .secondLogin)
    #expect(DesktopTitle.of(name: "vm", kind: .vm) == .numberedVM(1))
    #expect(DesktopTitle.of(name: "vm-2", kind: .vm) == .numberedVM(2))
    #expect(DesktopTitle.of(name: "vm-1", kind: .vm) == .named("vm-1"))
    #expect(DesktopTitle.of(name: "vm-02", kind: .vm) == .named("vm-02"))
    #expect(DesktopTitle.of(name: "work", kind: .vm) == .named("work"))
    #expect(DesktopTitle.of(name: "cloud", kind: .unknown("cloud")) == .named("cloud"))
  }

  @Test("a VM in maintenance stands in maintenance whatever its power, and a running one agents can't use needs setup")
  func standing() {
    #expect(DesktopStanding.of(vm(power: "running", standing: .idle)) == .running)
    #expect(DesktopStanding.of(vm(power: "running", standing: .notReady)) == .needsSetup)
    #expect(DesktopStanding.of(vm(power: "running", standing: .idle, maintenance: "on")) == .maintenance)
    #expect(DesktopStanding.of(vm(power: "running", standing: .idle, maintenance: "ending")) == .maintenance)
    #expect(
      DesktopStanding.of(vm(power: "stopped", standing: .notReady)) == .stopped,
      "a stopped VM starts when needed"
    )
    #expect(DesktopStanding.of(vm(power: "starting", standing: .notReady)) == .starting)
    #expect(DesktopStanding.of(vm(power: "stopping", standing: .idle)) == .stopping)
    #expect(DesktopStanding.of(vm(power: nil, standing: .idle)) == .unknown, "an unsaid power is never running")
    #expect(DesktopStanding.of(user(.notReady)) == .needsSetup)
    #expect(DesktopStanding.of(user(.working)) == .ready)
    #expect(DesktopStanding.of(user(nil)) == .unknown)
  }

  @Test("a VM's page shows only the rows it can do, in the page's order")
  func rows() {
    let running = vm(
      power: "running",
      standing: .idle,
      can: ["stop", "maintain", "remove"],
      facts: DesktopList.Facts(
        diskUsed: 28_000_000_000,
        diskSize: 128_000_000_000,
        macOS: "27.2.1",
        latestMacOS: "27.3",
        newerMacOS: true,
        atlas: "current"
      )
    )
    #expect(
      VMPageRows.of(running) == [
        .power(.running, button: .stop(enabled: true)),
        .maintenance(isOn: false, enabled: true),
        .disk(used: 28_000_000_000, size: 128_000_000_000),
        .macOS(version: "27.2.1", newer: "27.3"),
        .atlas(.current, next: nil),
        .remove,
      ]
    )
    #expect(VMPageRows.of(running).map(\.section) == [.use, .use, .about, .about, .about, .removal])

    let bare = vm(power: nil, standing: nil, can: [])
    #expect(VMPageRows.of(bare).isEmpty, "nothing said and nothing it can do shows no row")

    let same = vm(
      power: "stopped",
      standing: .notReady,
      can: ["start", "remove"],
      facts: DesktopList.Facts(
        macOS: "27.3",
        latestMacOS: "27.3",
        newerMacOS: false,
        atlas: "available",
        atlasNext: "Run atlas update."
      )
    )
    #expect(
      VMPageRows.of(same) == [
        .power(.stopped, button: .start(enabled: true)), .macOS(version: "27.3", newer: nil),
        .atlas(.available, next: "Run atlas update."), .remove,
      ],
      "no maintenance while it can be neither opened nor ended, and no newer macOS when it is not newer"
    )
  }

  @Test("maintenance rows follow the record: End… while it is on, and Stop… kept but waiting")
  func maintenance() {
    let on = vm(power: "running", standing: .idle, maintenance: "on", can: ["end", "remove"])
    #expect(
      VMPageRows.of(on).prefix(2) == [
        .power(.running, button: .stop(enabled: false)), .maintenance(isOn: true, enabled: true),
      ]
    )
    let ending = vm(power: "running", standing: .idle, maintenance: "ending", can: ["remove"])
    #expect(VMPageRows.of(ending).contains(.maintenance(isOn: true, enabled: false)))
    let stopping = vm(power: "stopping", standing: .idle, can: ["maintain", "remove"])
    #expect(VMPageRows.of(stopping).first == .power(.stopping, button: nil), "no button while it stops")
  }

  @Test("the sound row is hidden for a desktop without sound")
  func sound() {
    #expect(SoundRowRule.hasSound(.user))
    #expect(!SoundRowRule.hasSound(.vm))
    #expect(SoundRowRule.shows(nil), "before the list is read, Second Login is the one desktop")
    #expect(SoundRowRule.shows(DesktopList(desktops: [user(.idle), vm(power: "running", standing: .idle)])))
    #expect(!SoundRowRule.shows(DesktopList(desktops: [vm(power: "running", standing: .idle)])))
  }

  /// A VM named work, as `desktops.json` lists it.
  private func vm(
    power: String?,
    standing: OverviewState?,
    maintenance: String? = nil,
    can: [String] = [],
    facts: DesktopList.Facts = DesktopList.Facts()
  ) -> DesktopList.Entry {
    var said = facts
    said.power = power
    said.maintenance = maintenance ?? facts.maintenance
    return DesktopList.Entry(
      id: "vm-id",
      key: "vm-vm-id",
      name: "work",
      kind: .vm,
      standing: standing,
      can: can,
      facts: said
    )
  }

  /// Second Login, as `desktops.json` lists it.
  private func user(_ standing: OverviewState?) -> DesktopList.Entry {
    DesktopList.Entry(id: "user-id", key: "user", name: "main", kind: .user, isDefault: true, standing: standing)
  }
}
