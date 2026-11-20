import AtlasService

/// The desktops a test's service lists: Second Login and the VM work, each as `desktops.json` names it.
enum DesktopLists {
  /// Second Login, the default, doing `standing`.
  ///
  /// - Parameter standing: What it is doing; idle by default.
  /// - Returns: Its entry.
  static func secondLogin(_ standing: OverviewState? = .idle) -> DesktopList.Entry {
    DesktopList.Entry(
      id: "second-login",
      key: "user",
      name: "second-login",
      kind: .user,
      isDefault: true,
      standing: standing
    )
  }

  /// The VM work, running, doing `standing`, its maintenance as `maintenance` says.
  ///
  /// - Parameters:
  ///   - standing: What it is doing; working by default.
  ///   - power: Its power's word; running by default.
  ///   - maintenance: Where its maintenance stands; none by default.
  /// - Returns: Its entry.
  static func work(
    _ standing: OverviewState? = .working,
    power: String = "running",
    maintenance: String = "none"
  ) -> DesktopList.Entry {
    DesktopList.Entry(
      id: "work",
      key: "vm-work",
      name: "work",
      kind: .vm,
      standing: standing,
      can: ["stop", "maintain"],
      facts: DesktopList.Facts(power: power, maintenance: maintenance)
    )
  }

  /// Both, `shown` the one Atlas shows.
  ///
  /// - Parameters:
  ///   - shown: The ID of the desktop shown.
  ///   - secondLogin: Second Login's entry; idle by default.
  ///   - work: The VM's entry; working by default.
  /// - Returns: The list.
  static func two(
    shown: String,
    secondLogin: DesktopList.Entry = secondLogin(),
    work: DesktopList.Entry = work()
  ) -> DesktopList {
    var listed = [secondLogin, work]
    for index in listed.indices {
      listed[index].isViewed = listed[index].id == shown
    }
    return DesktopList(viewed: shown, desktops: listed)
  }

  /// Second Login alone, as a Mac with one desktop lists it.
  static let one = DesktopList(viewed: "second-login", desktops: [secondLogin()])
}
