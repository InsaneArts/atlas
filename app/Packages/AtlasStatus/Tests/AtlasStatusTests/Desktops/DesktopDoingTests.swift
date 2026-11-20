import AtlasService
import Testing

@testable import AtlasStatus

@Suite("What a desktop is doing")
struct DesktopDoingTests {
  @Test("a desktop not viewed is said by its newest status read, control there reading as in use elsewhere")
  func byItsStatus() {
    #expect(DesktopDoing.of(user(.working)) == .mode(.working))
    #expect(DesktopDoing.of(user(.idle)) == .mode(.idle))
    #expect(DesktopDoing.of(user(.control)) == .mode(.inUseElsewhere))
    #expect(DesktopDoing.of(user(.needsYou)) == .mode(.needsYou))
    #expect(DesktopDoing.of(user(.stopped)) == .mode(.stopped))
    #expect(DesktopDoing.of(user(.unknown("later"))) == .mode(.notReady))
    #expect(DesktopDoing.of(user(nil)) == .mode(.connecting), "nothing read yet says nothing")
    #expect(DesktopDoing.of(vm(power: "running", standing: .needsYou)) == .mode(.needsYou))
  }

  @Test("a VM that waits on its power or the person's maintenance asks nothing of the menu bar")
  func aWaitingVMIsNotUrgent() {
    let maintained = DesktopDoing.of(vm(power: "running", standing: .notReady, maintenance: "on"))
    #expect(maintained == .standing(.maintenance))
    #expect(maintained.kind == nil)
    #expect(DesktopDoing.of(vm(power: "stopped", standing: .notReady)) == .standing(.stopped))
    #expect(DesktopDoing.of(vm(power: "starting", standing: .notReady)).kind == nil)
    #expect(DesktopDoing.of(vm(power: nil, standing: .idle)) == .mode(.connecting), "an unsaid power is not running")
  }

  @Test("the menu bar dot shows the most urgent desktop, the first of two as urgent")
  func theMostUrgent() {
    #expect(Mode.Kind.mostUrgent([.idle, .working]) == .working)
    #expect(Mode.Kind.mostUrgent([.working, .needsYou, .stopped]) == .needsYou)
    #expect(Mode.Kind.mostUrgent([.control, .stopped]) == .stopped)
    #expect(Mode.Kind.mostUrgent([.connecting, .idle]) == .idle)
    #expect(Mode.Kind.mostUrgent([.notReady, .loggedOut]) == .loggedOut)
    #expect(Mode.Kind.mostUrgent([]) == nil)
    let ordered = Mode.Kind.allCases
      .sorted { $0.urgency > $1.urgency }
    #expect(Set(ordered.map(\.urgency)).count == Mode.Kind.allCases.count, "no two rows are as urgent")
  }

  /// A VM named work, as `desktops.json` lists it.
  ///
  /// - Parameters:
  ///   - power: Its power's word.
  ///   - standing: Its newest status read's state.
  ///   - maintenance: Where its maintenance stands; none by default.
  /// - Returns: The entry.
  private func vm(power: String?, standing: OverviewState?, maintenance: String? = nil) -> DesktopList.Entry {
    DesktopList.Entry(
      id: "vm-id",
      key: "vm-vm-id",
      name: "work",
      kind: .vm,
      standing: standing,
      facts: DesktopList.Facts(power: power, maintenance: maintenance)
    )
  }

  /// Second Login, as `desktops.json` lists it.
  ///
  /// - Parameter standing: Its newest status read's state.
  /// - Returns: The entry.
  private func user(_ standing: OverviewState?) -> DesktopList.Entry {
    DesktopList.Entry(
      id: "user-id",
      key: "user",
      name: "second-login",
      kind: .user,
      isDefault: true,
      standing: standing
    )
  }
}
