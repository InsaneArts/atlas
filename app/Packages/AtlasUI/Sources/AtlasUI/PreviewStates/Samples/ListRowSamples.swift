#if DEBUG
  import SwiftUI

  /// The list rows' samples, each in its grouped list at a setup column's width, as v3's Components block draws
  /// them (D6, D7).
  struct ListRowSamples: View {
    /// Which samples to show.
    enum Kind: Sendable, CaseIterable {
      /// Choose's accounts: the new account picked, with the helper's picture, and an account by its letter.
      case choices
      /// Each lead: none, a tile, the picture, a letter and a check's mark.
      case leads
      /// Each trailing kind: a word, a mark and a word, a switch, a button, a radio and a spinner.
      case trailing
      /// Each height: setup, Settings, Activity and a window row.
      case sizes
      /// Six accounts, scrolling past three and a half inside the group.
      case scrolling
    }

    /// Choose's two accounts, as v3's Components block draws them.
    static let accounts = [
      SampleListItem(
        id: 0,
        title: String("Create a new account"),
        detail: String("Recommended. A standard account named Atlas, with Atlas's picture."),
        lead: .picture
      ),
      SampleListItem(
        id: 1,
        title: String("Test Account"),
        detail: String("testaccount · standard account on this Mac"),
        lead: .letter(String("T"))
      ),
    ]

    /// A Settings group's row.
    static let settings = [
      SampleListItem(id: 0, title: String("Open Atlas at login"), detail: String("So Atlas is there whenever you are."))
    ]

    private let kind: Kind

    /// Creates the samples of one kind.
    ///
    /// - Parameter kind: Which samples to show.
    init(_ kind: Kind) {
      self.kind = kind
    }

    /// The samples, in their groups, on the ground.
    var body: some View {
      VStack(spacing: Spacing.m) {
        switch kind {
        case .choices:
          GroupedList(Self.accounts) { account in
            ListRow(account.title, detail: account.detail, lead: account.lead, trailing: .radio(isOn: account.id == 0))
          }
        case .leads:
          GroupedList(Self.leads) { row in
            ListRow(row.title, detail: row.detail, lead: row.lead, trailing: row.trailing)
          }
        case .trailing:
          GroupedList(Self.trailing) { row in
            ListRow(row.title, detail: row.detail, lead: row.lead, trailing: row.trailing)
          }
        case .sizes:
          ForEach(ListRow.Size.allCases, id: \.self) { size in
            GroupedList(Self.heights, style: size == .compact ? .compact : .regular) { row in
              ListRow(row.title, lead: row.lead, trailing: .text(String("\(Int(size.height)) pt")), size: size)
            }
          }
        case .scrolling:
          GroupedList(Self.many, scrolling: .regular) { account in
            ListRow(account.title, detail: account.detail, lead: account.lead, trailing: .radio(isOn: account.id == 0))
          }
        }
      }
      .frame(width: SampleWidth.column)
      .sampleSurface()
    }

    /// Two rows for the heights: one with nothing leading, one with a check's mark.
    private static let heights = [
      SampleListItem(id: 0, title: String("Open Atlas at login")),
      SampleListItem(id: 1, title: String("Atlas runs there"), lead: .check(.done)),
    ]

    /// One row of each lead.
    private static let leads = [
      SampleListItem(id: 0, title: String("Open Atlas at login"), detail: String("Nothing leads it.")),
      SampleListItem(
        id: 1,
        title: String("Accessibility"),
        detail: String("So agents can click and type"),
        lead: .tile(.symbol(SymbolName.click.rawValue))
      ),
      SampleListItem(id: 2, title: String("Atlas"), detail: String("The helper account"), lead: .picture),
      SampleListItem(id: 3, title: String("Test Account"), detail: String("testaccount"), lead: .letter(String("T"))),
      SampleListItem(
        id: 4,
        title: String("The Atlas account is logged in"),
        lead: .check(.done),
        trailing: .text(String("in the background"))
      ),
      SampleListItem(
        id: 5,
        title: String("A test screenshot"),
        lead: .check(.failed),
        trailing: .text(String("failed"), tone: .problem)
      ),
    ]

    /// One row of each trailing kind.
    private static let trailing = [
      SampleListItem(
        id: 0,
        title: String("Screen Recording"),
        detail: String("So agents can see"),
        lead: .tile(.symbol(SymbolName.display.rawValue)),
        trailing: .text(String("Off"))
      ),
      SampleListItem(
        id: 1,
        title: String("Accessibility"),
        detail: String("So agents can click and type"),
        lead: .tile(.symbol(SymbolName.click.rawValue)),
        trailing: .check(.done, String("Allowed"))
      ),
      SampleListItem(
        id: 2,
        title: String("Codex"),
        detail: String("Found."),
        lead: .tile(.text(String(">_"))),
        trailing: .toggle(isOn: true)
      ),
      SampleListItem(id: 3, title: String("Atlas runs there"), trailing: .button(String("Check Again"))),
      SampleListItem(id: 4, title: String("Test Account"), lead: .letter(String("T")), trailing: .radio(isOn: false)),
      SampleListItem(id: 5, title: String("Claude Code"), lead: .tile(.text(String(">_"))), trailing: .progress),
    ]

    /// Six accounts, for the scrolling sample.
    private static let many =
      accounts
      + (2..<6)
      .map { index in
        SampleListItem(
          id: index,
          title: String("Account \(index)"),
          detail: String("account\(index) · standard account on this Mac"),
          lead: .letter(String("\(index)"))
        )
      }
  }
#endif
