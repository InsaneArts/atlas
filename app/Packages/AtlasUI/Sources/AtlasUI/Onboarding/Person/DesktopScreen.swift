import SwiftUI

/// Choose where agents work: Second Login, a VM or Cloud, in one group whose radio is the only mark of the choice. A
/// kind this Mac cannot have now is dimmed, with why at its end in place of a radio, and is no button: no pointer fill,
/// no press and no focus. Continue is the footer's.
struct DesktopScreen: View {
  /// The rows to choose from.
  let state: OnboardingDesktopScreenState
  /// Receives the kind picked.
  let send: @MainActor (OnboardingEvent) -> Void

  /// What a desktop of their own is, then the rows.
  var body: some View {
    ScreenColumn {
      OnboardingHeader(
        label: "Desktop",
        title: OnboardingScreen.desktopTitle,
        tail: OnboardingScreen.desktopTail,
        lede: "They open and click through Mac apps there, on a desktop of their own. Your screen stays yours."
      )
      GroupedList(Self.rows(state)) { row in
        ListRow(
          row.title,
          detail: row.detail,
          tag: row.tag,
          tagTone: row.tagTone,
          lead: row.lead,
          trailing: row.trailing,
          isNotYet: row.isNotYet,
          action: row.isNotYet ? nil : { send(.pickDesktop(row.id)) }
        )
      }
      .padding(.top, OnboardingLayout.Rhythm.paragraph)
    }
  }

  /// Each kind's row: its symbol on a tile, its name, a grey Beta on Second Login and VM, and its line; a radio when
  /// it can be picked, else dimmed with why at its end.
  ///
  /// - Parameter state: The choice.
  /// - Returns: The rows, in order.
  static func rows(_ state: OnboardingDesktopScreenState) -> [ListRowItem<OnboardingDesktopKind>] {
    state.items.map { item in
      let (trailing, isNotYet): (ListRow.Trailing, Bool) =
        switch item.standing {
        case .choice(let isPicked): (.radio(isOn: isPicked), false)
        case .notYet(let why): (.text(why), true)
        }
      return ListRowItem(
        id: item.id,
        title: title(item.id),
        detail: detail(item.id),
        tag: item.id == .cloud ? nil : "Beta",
        tagTone: .quiet,
        lead: .tile(.symbol(symbol(item.id))),
        trailing: trailing,
        isNotYet: isNotYet
      )
    }
  }

  /// A kind's name.
  ///
  /// - Parameter kind: The kind of desktop.
  /// - Returns: Its name.
  private static func title(_ kind: OnboardingDesktopKind) -> String {
    switch kind {
    case .secondLogin: "Second Login"
    case .vm: "VM"
    case .cloud: "Cloud"
    }
  }

  /// A kind's line under its name.
  ///
  /// - Parameter kind: The kind of desktop.
  /// - Returns: What it is.
  private static func detail(_ kind: OnboardingDesktopKind) -> String {
    switch kind {
    case .secondLogin: "A hidden second login on this Mac."
    case .vm: "A virtual Mac on this Mac."
    case .cloud: "A Mac in the cloud."
    }
  }

  /// A kind's symbol on its tile.
  ///
  /// - Parameter kind: The kind of desktop.
  /// - Returns: The SF Symbol's name.
  private static func symbol(_ kind: OnboardingDesktopKind) -> String {
    switch kind {
    case .secondLogin: "person.crop.circle"
    case .vm: "macwindow"
    case .cloud: "cloud"
    }
  }
}

#if DEBUG
  #Preview("Desktop, VM available") {
    DesktopScreen(state: PreviewStates.onboardingDesktopScreen) { _ in }
      .onboardingSample()
  }

  #Preview("Desktop, VM not available") {
    DesktopScreen(state: PreviewStates.onboardingDesktopNoVMScreen) { _ in }
      .onboardingSample()
  }
#endif
