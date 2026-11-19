import AtlasCommands
import AtlasService
import AtlasUI

/// The desktops: the list read while the window shows, a VM's page, and what the person does there. Start and Stop go
/// through the service as the person's own, Stop asking first; maintenance and removal are `atlas desktop` commands
/// the approval window confirms. One VM command runs at a time on its page, its buttons waiting meanwhile, and the list
/// is read again once it ends.
extension SettingsController {
  /// How often the desktops are read while the window shows: 3 s, so a VM starting, stopping or opened for
  /// maintenance shows soon.
  static let desktopsEvery: Duration = .seconds(3)

  /// Reads the desktops now, then every ``desktopsEvery`` while the window shows; never two readings at once.
  func followDesktops() {
    guard desktopsReading == nil else { return }
    let clock = dependencies.clock
    desktopsReading = Task { [weak self] in
      while !Task.isCancelled {
        // The controller is held for the reading alone, never through the pause.
        guard let controller = self else { return }
        await controller.readDesktops()
        try? await clock.sleep(for: Self.desktopsEvery)
      }
    }
  }

  /// Reads the desktops once, keeping the last list while the service cannot say: the page of a desktop no longer
  /// listed goes back to the page in Helper's place.
  func readDesktops() async {
    guard let list = await dependencies.links.desktops() else { return }
    if list != desktops { desktops = list }
    let route = session.route.kept(desktops: list.desktops.map(\.id))
    if route != session.route, !isAsking { session.route = route }
  }

  /// The VM listed with `id`, by its name in the settings; nil when none is.
  ///
  /// - Parameter id: The VM's ID.
  /// - Returns: Its entry.
  func listedVM(_ id: String) -> DesktopList.Entry? {
    desktops?.desktops.first { $0.id == id && $0.kind == .vm }
  }

  /// The route a VM's page shows on: its own page, or the page in Helper's place while it is the one desktop.
  ///
  /// - Parameter id: The VM's ID.
  /// - Returns: The route.
  func route(ofVM id: String) -> SettingsRoute {
    desktops?.desktops.count == 1 ? .page(.helper) : .desktop(id)
  }

  /// Start: the service starts the VM as an agent's command would.
  ///
  /// - Parameter id: The VM's ID.
  func start(_ id: String) async {
    guard let vm = listedVM(id) else { return }
    let title = SettingsWords.title(of: vm)
    await working(on: id, waiting: SettingsWords.starting(title)) {
      do {
        try await dependencies.links.setPower(id, true)
        say(nil, on: route(ofVM: id))
      } catch {
        say(
          SettingsWords.powerFailed(title, start: true, error.localizedDescription),
          on: route(ofVM: id),
          kind: .problem
        )
      }
    }
  }

  /// Stop, once asked: the service stops the VM, ending work there, and keeps the stop until Atlas VM confirms it.
  ///
  /// - Parameter id: The VM's ID.
  func stop(_ id: String) async {
    guard let vm = listedVM(id) else { return }
    let title = SettingsWords.title(of: vm)
    await working(on: id, waiting: nil) {
      do {
        try await dependencies.links.setPower(id, false)
        say(nil, on: route(ofVM: id))
      } catch {
        say(
          SettingsWords.powerFailed(title, start: false, error.localizedDescription),
          on: route(ofVM: id),
          kind: .problem
        )
      }
    }
  }

  /// Open for Maintenance… or End…: `atlas desktop maintain`, which the approval window confirms.
  ///
  /// - Parameters:
  ///   - id: The VM's ID.
  ///   - end: Whether it ends the maintenance.
  func maintain(_ id: String, end: Bool) async {
    guard let vm = listedVM(id) else { return }
    await working(on: id, waiting: SettingsWords.approving) {
      let outcome = await dependencies.commands.run(.maintainVM(vm.name, end: end))
      say(outcome.ok ? nil : outcome.message, on: route(ofVM: id), kind: .problem)
    }
  }

  /// Remove…: `atlas desktop remove`, which the approval window confirms. Once removed, the window goes back to the
  /// page in Helper's place, which says so.
  ///
  /// - Parameter id: The VM's ID.
  func remove(_ id: String) async {
    guard let vm = listedVM(id) else { return }
    let title = SettingsWords.title(of: vm)
    await working(on: id, waiting: SettingsWords.approving) {
      let outcome = await dependencies.commands.run(.removeVM(vm.name))
      guard outcome.ok else { return say(outcome.message, on: route(ofVM: id), kind: .problem) }
      if !isAsking { session.route = .page(.helper) }
      say(SettingsWords.removed(title), in: .helper)
    }
  }

  /// Runs one command of the VM `id`'s page, its buttons waiting meanwhile under `waiting`, then reads the desktops
  /// again; none while another of its commands runs.
  ///
  /// - Parameters:
  ///   - id: The VM's ID.
  ///   - waiting: What it waits on, in words; nil for none, as while an alert shows it.
  ///   - command: The command.
  private func working(on id: String, waiting: String?, _ command: () async -> Void) async {
    guard session.desktopWorking == nil else { return }
    session.desktopWorking = id
    session.desktopWaiting = waiting
    await command()
    await readDesktops()
    session.desktopWorking = nil
    session.desktopWaiting = nil
  }
}
