import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import AtlasService
import AtlasUI
import Testing

@testable import AtlasApp

@Suite("Settings desktops")
struct SettingsDesktopsTests {
  /// Second Login, the default, as `desktops.json` lists it.
  static let secondLogin = DesktopList.Entry(
    id: "user-id",
    key: "user",
    name: "main",
    kind: .user,
    isDefault: true,
    standing: .idle
  )

  /// The VM work, running, as `desktops.json` lists it.
  static let work = DesktopList.Entry(
    id: "vm-id",
    key: "vm-vm-id",
    name: "work",
    kind: .vm,
    standing: .idle,
    can: ["stop", "maintain", "remove"],
    facts: DesktopList.Facts(
      power: "running",
      display: ViewerStatus.Size(width: 1920, height: 1200),
      diskUsed: 28_000_000_000,
      diskSize: 128_000_000_000,
      macOS: "27.2.1",
      latestMacOS: "27.3",
      newerMacOS: true,
      maintenance: "none",
      atlas: "current"
    )
  )

  /// A snapshot listing `desktops`, on `route`.
  private func snapshot(_ desktops: [DesktopList.Entry], route: SettingsRoute = .page(.helper)) -> SettingsSnapshot {
    var snapshot = SettingsSnapshot()
    snapshot.desktops = DesktopList(viewed: desktops.first?.id, desktops: desktops)
    snapshot.session.route = route
    return snapshot
  }

  @Test("with one desktop its page opens directly with Add a Desktop at its foot")
  func oneDesktop() throws {
    let helper = SettingsPresenter.state(snapshot([Self.secondLogin]))
    #expect(helper.desktops == nil)
    #expect(helper.vm == nil)
    #expect(helper.helper.addsDesktop)
    #expect(helper.sidebar.rows.first?.title == "Helper")

    let vm = SettingsPresenter.state(snapshot([Self.work]))
    #expect(vm.desktops == nil)
    let page = try #require(vm.vm, "the one VM's page shows in Helper's place")
    #expect(page.addsDesktop)
    #expect(vm.sidebar.rows.first?.title == "work")
    #expect(vm.title == "work")

    let unread = SettingsPresenter.state(SettingsSnapshot())
    #expect(!unread.helper.addsDesktop, "nothing offered before the desktops are read")
  }

  @Test("with two desktops the first row is Desktops, listing each with the default tagged")
  func twoDesktops() throws {
    let state = SettingsPresenter.state(snapshot([Self.secondLogin, Self.work]))
    let first = try #require(state.sidebar.rows.first)
    #expect(first.page == .helper)
    #expect(first.title == "Desktops")
    #expect(first.subtitle == "Ready")
    #expect(state.title == "Desktops")
    let desktops = try #require(state.desktops)
    #expect(
      desktops.rows == [
        SettingsDesktopRow(
          id: "user-id",
          title: "Second Login",
          detail: "A hidden second login on this Mac",
          isDefault: true,
          isVM: false
        ),
        SettingsDesktopRow(id: "vm-id", title: "work", detail: "A virtual Mac on this Mac · Running", isVM: true),
      ]
    )
    #expect(!state.helper.addsDesktop)

    var needing = Self.work
    needing.standing = .notReady
    let named = SettingsPresenter.state(snapshot([Self.secondLogin, needing]))
    #expect(named.sidebar.rows.first?.subtitle == "work needs setup")
    #expect(named.sidebar.rows.first?.tone == .attention)
  }

  @Test("the Desktops list says In maintenance while the person has a VM open")
  func inMaintenance() throws {
    var maintained = Self.work
    maintained.facts?.maintenance = "on"
    maintained.can = ["end", "remove"]
    let state = SettingsPresenter.state(snapshot([Self.secondLogin, maintained]))
    let row = try #require(state.desktops?.rows.last)
    #expect(row.detail == "A virtual Mac on this Mac · In maintenance")
  }

  @Test("Second Login's page is unchanged, opened from Desktops or as the one desktop")
  func secondLoginUnchanged() {
    let alone = SettingsPresenter.state(SettingsSnapshot())
    var listed = SettingsPresenter.state(snapshot([Self.secondLogin, Self.work], route: .desktop("user-id")))
    #expect(listed.vm == nil, "Second Login's page is the Helper page")
    #expect(listed.title == "Second Login")
    #expect(listed.helper == alone.helper)
    listed = SettingsPresenter.state(snapshot([Self.secondLogin]))
    var added = listed.helper
    added.addsDesktop = false
    #expect(added == alone.helper, "only Add a Desktop… is new")
  }

  @Test("a VM's page shows only the rows it can do, worded, in sections under their headings")
  func vmPage() throws {
    let state = SettingsPresenter.state(snapshot([Self.secondLogin, Self.work], route: .desktop("vm-id")))
    let page = try #require(state.vm)
    #expect(state.title == "work")
    #expect(page.line == "A virtual Mac on this Mac · 1920 × 1200")
    #expect(page.tone == .ready)
    #expect(page.sections.map(\.title) == ["Power and Maintenance", "Disk and Software", nil])
    let rows = page.sections.flatMap(\.rows)
    #expect(
      rows.map(\.title) == [
        "Running", "Open for maintenance", "Disk", "macOS 27.2.1", "Atlas in the VM", "Remove this VM",
      ]
    )
    #expect(rows[0].button == SettingsVMButton(title: "Stop…", event: .askStopVM("vm-id")))
    #expect(rows[1].button == SettingsVMButton(title: "Open…", event: .maintainVM("vm-id")))
    #expect(rows[2].value == "28 GB used of 128 GB")
    #expect(rows[3].detail == "macOS 27.3 is available. Open the VM for maintenance and update it in Software Update.")
    #expect(rows[4].value == "Up to date")
    #expect(rows[5].button == SettingsVMButton(title: "Remove…", event: .removeVM("vm-id"), isDestructive: true))
    #expect(!page.addsDesktop)
  }

  @Test("maintenance rows follow the record: End… on, Stop… waiting and the dot blue")
  func maintenanceRows() throws {
    var maintained = Self.work
    maintained.facts?.maintenance = "on"
    maintained.can = ["end", "remove"]
    let state = SettingsPresenter.state(snapshot([Self.secondLogin, maintained], route: .desktop("vm-id")))
    let page = try #require(state.vm)
    #expect(page.tone == .link)
    let rows = page.sections.flatMap(\.rows)
    #expect(rows[0].button?.isEnabled == false)
    #expect(rows[1].detail == "Agents wait until you end it")
    #expect(rows[1].button == SettingsVMButton(title: "End…", event: .endMaintenance("vm-id")))
  }

  @Test("the sound row is hidden for a desktop without sound")
  func soundRow() {
    #expect(SettingsPresenter.general(SettingsSnapshot()).hasSound)
    #expect(SettingsPresenter.general(snapshot([Self.secondLogin, Self.work])).hasSound)
    #expect(!SettingsPresenter.general(snapshot([Self.work])).hasSound)
  }

  @Test("a desktop's page no longer listed goes back to Desktops")
  func removedRoute() {
    let state = SettingsPresenter.state(snapshot([Self.secondLogin, Self.work], route: .desktop("gone")))
    #expect(state.route == .page(.helper))
    #expect(state.vm == nil)
  }

  @Test("Stop asks first and names the VM, then stops it through the service as the person")
  @MainActor
  func stopAsksFirst() async throws {
    let powered = PowerLog()
    let controller = SettingsController.scripted(
      commands: ScriptedCommands([:]),
      edit: { dependencies in
        dependencies.links.desktops = { DesktopList(viewed: "user-id", desktops: [Self.secondLogin, Self.work]) }
        dependencies.links.setPower = { id, on in powered.calls.append("\(id) \(on ? "on" : "off")") }
      }
    )
    await controller.readDesktops()
    controller.send(.openDesktop("vm-id"))
    controller.send(.askStopVM("vm-id"))
    let alert = try #require(controller.state.alert)
    #expect(alert.title == "Stop work?")
    #expect(alert.message == "Anything agents are doing there stops. It starts again when an agent needs it.")
    #expect(powered.calls.isEmpty, "nothing stops before the person confirms")
    await controller.confirmAlert()
    #expect(powered.calls == ["vm-id off"])
    #expect(controller.state.alert == nil)
  }

  @Test("Open… and End… run atlas desktop maintain, which the approval window confirms, and Remove… goes back")
  @MainActor
  func maintainAndRemove() async throws {
    let commands = ScriptedCommands(["desktop maintain": #"{"ok": true}"#, "desktop remove": #"{"ok": true}"#])
    let controller = SettingsController.scripted(
      commands: commands,
      edit: { dependencies in
        dependencies.links.desktops = { DesktopList(viewed: "user-id", desktops: [Self.secondLogin, Self.work]) }
      }
    )
    await controller.readDesktops()
    controller.send(.openDesktop("vm-id"))
    await controller.maintain("vm-id", end: false)
    await controller.maintain("vm-id", end: true)
    await controller.remove("vm-id")
    #expect(
      commands.ran == [
        Action.maintainVM("work", end: false).arguments, Action.maintainVM("work", end: true).arguments,
        Action.removeVM("work").arguments,
      ]
    )
    #expect(controller.session.route == .page(.helper))
    #expect(controller.state.desktops?.notice?.text == "work was removed.")
  }

  @Test("Add a Desktop… opens the setup window on its choice")
  @MainActor
  func addDesktop() {
    let opened = CallCount()
    let controller = SettingsController.scripted(
      commands: ScriptedCommands([:]),
      edit: { dependencies in
        dependencies.links.addDesktop = { opened.count += 1 }
      }
    )
    controller.send(.addDesktop)
    #expect(opened.count == 1)
  }
}
