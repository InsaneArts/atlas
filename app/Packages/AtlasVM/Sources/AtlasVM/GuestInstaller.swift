import AtlasVMKit
import Darwin
import Foundation

/// Putting Atlas inside the VM's guest, once: over Remote Login, phase by
/// phase, each safe to repeat, its progress kept in the VM's folder, and the
/// guest itself changed only by Atlas's own `machine-setup`, as root.
actor GuestInstaller {
  private let folder: VMFolder
  private let desktop: String
  private let machine: Machine

  /// The guest's own account, and the second administrator's.
  private static let account = "atlas"
  private static let admin = "atlas-admin"
  /// The codes of a setup phase's refusals that asking again cannot change.
  private static let forGood: Set<String> = ["bad-request", "identity"]

  /// The installer of the VM in `folder`, the desktop `desktop`, run by `machine`.
  init(folder: VMFolder, desktop: String, machine: Machine) {
    self.folder = folder
    self.desktop = desktop
    self.machine = machine
  }

  private var progressFile: URL { folder.root.appending(path: "Bootstrap.json") }
  private var bootstrap: URL { folder.root.appending(path: "Bootstrap", directoryHint: .isDirectory) }
  private var archive: URL { bootstrap.appending(path: "Atlas.zip") }
  private var shared: URL { folder.share.appending(path: "pairing-secret") }

  /// Install for the change `change`, as far as it goes: done once the
  /// station said it is closed; none while it waits on that word, on being
  /// asked again after a failure that may pass, or when its progress cannot
  /// be read; a refusal for good otherwise, once the VM is quarantined and
  /// stopped and the secret has left the share. One install at a time.
  func install(_ change: ChangeID, _ request: InstallRequest) async -> ChangeOutcome? {
    let kept: InstallProgress?
    do {
      kept = try InstallProgress.read(from: progressFile)
    } catch {
      return nil
    }
    // Refused already: answered so once cleaned up, its cleanup going on
    // until then, the VM kept quarantined.
    if let kept, kept.change == change, let why = kept.refused {
      return kept.ended ? .refused(why) : await refuse(kept, why: why)
    }
    if let kept, kept.change != change, !kept.passed(.closed), !kept.ended {
      return .refused("Another install is under way in this VM.")
    }
    if kept?.change == change, kept?.passed(.closed) == true { return .done }
    await machine.release()
    // Read again past that wait, so a report taken meanwhile holds.
    let current = (try? InstallProgress.read(from: progressFile)) ?? nil
    var progress = current.flatMap { $0.change == change ? $0 : nil } ?? InstallProgress(change: change)
    do {
      try await run(&progress, request)
      return nil
    } catch let refusal as InstallRefusal {
      return await refuse(progress, why: refusal.why)
    } catch {
      // It may pass: the install stays pending, and is asked again, with
      // why it stopped kept to be read.
      progress.failure = String(String(describing: error).prefix(500))
      try? persist(&progress)
      return nil
    }
  }

  /// Refuse the install of `progress` for good, its refusal kept first,
  /// once the VM is kept quarantined and is stopped, and the secret has
  /// left the share; until then it stays pending, and asked again goes on
  /// cleaning up. Its progress is kept, ended, so asked again it answers
  /// the same, and a later install may start afresh.
  private func refuse(_ progress: InstallProgress, why: String) async -> ChangeOutcome? {
    var progress = progress
    progress.refused = why
    do {
      try persist(&progress)
      try await machine.quarantine()
      try await machine.stop()
      guard await machine.power().value == .stopped else { throw RemoteFailure(description: "The VM is not stopped.") }
      try PrivateFile.remove(shared)
      progress.ended = true
      try persist(&progress)
    } catch {
      progress.failure = "Refused, but not yet cleaned up: \(why)"
      try? persist(&progress)
      return nil
    }
    return .refused(why)
  }

  private func keep(_ progress: inout InstallProgress, _ phase: InstallPhase) throws {
    progress.done = phase
    progress.failure = nil
    try persist(&progress)
  }

  /// Keep `progress`, with what the station's reports said while this
  /// install waited never lost: that the guest paired, and how far it got.
  private func persist(_ progress: inout InstallProgress) throws {
    if let kept = try? InstallProgress.read(from: progressFile), kept.change == progress.change {
      progress.paired = progress.paired || kept.paired
      if let further = kept.done, !progress.passed(further) {
        progress.done = further
      }
    }
    try progress.write(to: progressFile)
  }

  private func run(_ progress: inout InstallProgress, _ request: InstallRequest) async throws {
    let change = progress.change
    if !progress.passed(.archived) {
      try VMFolder.makePrivate(bootstrap)
      try? FileManager.default
        .removeItem(at: archive)
      try tool("/usr/bin/ditto", ["-c", "-k", "--keepParent", request.bundle, archive.path(percentEncoded: false)])
      progress.archive = try digest(of: archive)
      try keep(&progress, .archived)
    }
    // Frozen: every try sends that very archive.
    guard try digest(of: archive) == progress.archive else {
      throw InstallRefusal(why: "The archive of Atlas changed.")
    }
    if !progress.paired {
      try PrivateFile.write(Data(request.secret.utf8), to: shared)
    }
    guard let address = await guestAddress() else { throw RemoteFailure(description: "The guest has no address yet.") }
    // A report taken while this waited holds: a guest that paired needs no secret.
    if let kept = try? InstallProgress.read(from: progressFile), kept.change == change {
      progress.paired = progress.paired || kept.paired
    }
    let atlas = Remote(user: Self.account, address: address, password: try GuestPassword.of(desktop), folder: folder)
    let staging = "~/.atlas-bootstrap/\(change.text)"
    if !progress.passed(.uploaded) {
      _ = try atlas.run(
        "umask 077 && mkdir -p ~/.atlas-bootstrap && cat > ~/.atlas-bootstrap/Atlas.zip",
        input: try Data(contentsOf: archive)
      )
      _ = try atlas.run("rm -rf \(staging) && mkdir \(staging) && ditto -x -k ~/.atlas-bootstrap/Atlas.zip \(staging)")
      try keep(&progress, .uploaded)
    }
    guard let requirement = CodeIdentity.atlasRequirement() else {
      throw InstallRefusal(why: "Atlas VM's own signature cannot be read.")
    }
    let root = { (arguments: String) in Self.asRoot(change, requirement: requirement, arguments) }
    if !progress.passed(.published) {
      try setup(
        atlas,
        root("publish --change \(change.text) --source ~\(Self.account)/.atlas-bootstrap/\(change.text)/Atlas.app")
      )
      try keep(&progress, .published)
    }
    if !progress.passed(.jobs) {
      let paired = progress.paired ? " --paired" : ""
      try setup(
        atlas,
        root("jobs --desktop-id \(desktop) --enrolment \(request.enrolment) --account \(Self.account)\(paired)")
      )
      try keep(&progress, .jobs)
    }
    let adminPassword = try GuestPassword.of("\(desktop).admin")
    if !progress.passed(.admin) {
      try setup(atlas, root("admin --name \(Self.admin)"), payload: adminPassword)
      try keep(&progress, .admin)
    }
    // The second administrator proved to sign in and to pass sudo before
    // anything relies on it.
    let admin = Remote(user: Self.admin, address: address, password: adminPassword, folder: folder)
    if !progress.passed(.demoted) {
      _ = try admin.sudo("-v")
      try setup(admin, root("demote --account \(Self.account)"))
      try keep(&progress, .demoted)
    }
    // Closed only once the guest daemon answers on its port, and kept as
    // closing before it is asked, so a run cut short after it still takes
    // the station's word.
    if !progress.passed(.closing) {
      guard await machine.guestAnswers() else {
        throw RemoteFailure(description: "The guest daemon does not answer yet.")
      }
      try keep(&progress, .closing)
    }
    // Asked again until the station says closed: closed again while Remote
    // Login still answers, which is safe to repeat; once it does not, done.
    do {
      try setup(admin, root("close"))
    } catch let failure as RemoteFailure where failure.unreachable {}
  }

  /// The command `sudo` runs in the guest for `atlas machine-setup` with
  /// `arguments`, plain words alone: as root, from a copy of the Atlas
  /// staged for the install `change` that no one but root can change, made
  /// from the stage when it is missing or not whole, and checked each time
  /// by the system's own `codesign` to be Atlas, signed as `requirement`
  /// says, before it runs. Every phase runs so, never from a place an
  /// administrator could replace.
  static func asRoot(_ change: ChangeID, requirement: String, _ arguments: String) -> String {
    let quoted = requirement.replacingOccurrences(of: "\"", with: "\\\"")
    let refusal = #"{\"ok\":false,\"code\":\"identity\",\"error\":\"The staged Atlas is not signed as Atlas.\"}"#
    let script = """
      made=/var/root/.atlas-setup-\(change.text)
      staged=~\(account)/.atlas-bootstrap/\(change.text)/Atlas.app
      checked() { /usr/bin/codesign --verify --strict --deep -R "=\(quoted)" "$made/Atlas.app" 2>/dev/null; }
      if ! checked; then
        rm -rf "$made"
        (umask 077 && mkdir "$made") || exit 1
        if ! /usr/bin/ditto "$staged" "$made/Atlas.app" || ! checked; then
          rm -rf "$made"
          echo "\(refusal)"
          exit 1
        fi
      fi
      exec "$made/Atlas.app/Contents/MacOS/atlas" machine-setup "$@"
      """
    return "/bin/sh -c '\(script)' setup \(arguments)"
  }

  /// Take the station's word that the guest paired: its secret leaves the
  /// share. None while that cannot be read or kept.
  func paired(install: ChangeID) -> ChangeOutcome? {
    let kept: InstallProgress?
    do {
      kept = try InstallProgress.read(from: progressFile)
    } catch {
      return nil
    }
    guard var progress = kept, progress.change == install else { return .refused("No such install.") }
    progress.paired = true
    do {
      try progress.write(to: progressFile)
      try PrivateFile.remove(shared)
    } catch {
      return nil
    }
    return .done
  }

  /// Take the station's word that the guest is closed. None while that
  /// cannot be read or kept.
  func closed(install: ChangeID) -> ChangeOutcome? {
    let kept: InstallProgress?
    do {
      kept = try InstallProgress.read(from: progressFile)
    } catch {
      return nil
    }
    guard var progress = kept, progress.change == install, progress.passed(.closing) else {
      return .refused("That install is not closing.")
    }
    progress.done = .closed
    do {
      try progress.write(to: progressFile)
      try PrivateFile.remove(shared)
    } catch {
      return nil
    }
    return .done
  }

  /// Where the guest answers: its address from the Mac's DHCP leases for
  /// this VM's own MAC address.
  private func guestAddress() async -> String? {
    guard let mac = await machine.macAddress,
      let leases = try? String(contentsOfFile: "/var/db/dhcpd_leases", encoding: .utf8)
    else {
      return nil
    }
    return DHCPLeases.address(of: mac, in: leases)
  }

  /// Run the setup phase `command` as root in the guest, as `remote`, and
  /// go on only if its answer says it held, even when it failed.
  private func setup(_ remote: Remote, _ command: String, payload: String? = nil) throws {
    let printed: Data
    do {
      printed = try remote.sudo(command, payload: payload)
    } catch let failure as RemoteFailure where !failure.printed.isEmpty {
      printed = failure.printed
    }
    try answered(printed)
  }

  /// Go on only if a setup phase's answer says it held. A refusal that
  /// asking again cannot change is for good; any other failure may pass.
  private func answered(_ printed: Data) throws {
    let line =
      String(decoding: printed, as: UTF8.self)
      .split(separator: "\n")
      .last
      .map(String.init) ?? ""
    guard let reply = try? JSONSerialization.jsonObject(with: Data(line.utf8)) as? [String: Any] else {
      throw RemoteFailure(description: "A setup phase did not answer.")
    }
    guard reply["ok"] as? Bool == true else {
      let why = (reply["error"] as? String) ?? "A setup phase refused."
      guard let code = reply["code"] as? String, Self.forGood.contains(code) else {
        throw RemoteFailure(description: why)
      }
      throw InstallRefusal(why: why)
    }
  }

  private func tool(_ program: String, _ arguments: [String]) throws {
    let running = Process()
    running.executableURL = URL(filePath: program)
    running.arguments = arguments
    running.standardOutput = FileHandle.nullDevice
    running.standardError = FileHandle.nullDevice
    try running.run()
    running.waitUntilExit()
    guard running.terminationStatus == 0 else { throw InstallRefusal(why: "Atlas could not be archived.") }
  }

  private func digest(of file: URL) throws -> String {
    let running = Process()
    running.executableURL = URL(filePath: "/usr/bin/shasum")
    running.arguments = ["-a", "256", file.path(percentEncoded: false)]
    let output = Pipe()
    running.standardOutput = output
    try running.run()
    let printed = output.fileHandleForReading.readDataToEndOfFile()
    running.waitUntilExit()
    return String(decoding: printed, as: UTF8.self)
      .split(separator: " ").first
      .map(String.init) ?? ""
  }
}
