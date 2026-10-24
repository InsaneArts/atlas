#if DEBUG
  import SwiftUI

  /// Settings' sections at a form's width (G3, G5–G8): rows with a switch and a button under no name, then a named
  /// section of checks, one failing with its fix, and a project's path, with each kind of note under it.
  struct GroupedSectionSamples: View {
    /// A section of controls.
    static let controls = [
      SampleListItem(
        id: 0,
        title: String("Open Atlas at login"),
        detail: String("Atlas sits in the menu bar from the start"),
        trailing: .toggle(isOn: true)
      ),
      SampleListItem(
        id: 1,
        title: String("Recover"),
        detail: String("Checks the helper and starts its mouse and keyboard again"),
        trailing: .button(String("Recover…"))
      ),
    ]

    /// A section of checks.
    static let checks = [
      SampleListItem(id: 0, title: String("Helper is running"), lead: .check(.done)),
      SampleListItem(
        id: 1,
        title: String("Mouse and keyboard"),
        detail: String("Recover starts the helper's mouse and keyboard again."),
        lead: .check(.failed)
      ),
      SampleListItem(id: 2, title: String("Save and Open dialogs"), detail: String("Not checked yet"), lead: .pending),
    ]

    /// What it draws, from its state.
    var body: some View {
      VStack(alignment: .leading, spacing: SettingsLayout.sectionGap) {
        GroupedSection(Self.controls) { item in
          ListRow(item.title, detail: item.detail, trailing: item.trailing, size: .compact) {}
        }
        GroupedSection(
          Self.checks,
          title: String("Ready to act"),
          notes: [
            .status(.working, String("Another action is running. Try again once it ends.")),
            .status(.problem, String("Claude app: its settings file is not valid JSON.")),
            .text(String("Forget takes Atlas's section out.")),
          ]
        ) { item in
          ListRow(
            item.title,
            detail: item.detail,
            detailStyle: item.lead == .check(.failed) ? .problem : .plain,
            lead: item.lead,
            size: .compact
          )
        }
        GroupedSection([SampleListItem(id: 0, title: String("opie"), detail: String("~/dev/opie"))]) { item in
          ListRow(
            item.title,
            detail: item.detail,
            detailStyle: .path,
            lead: .tile(.symbol(SymbolName.folder.rawValue)),
            trailing: .button(String("Forget")),
            size: .compact
          ) {}
        }
      }
      .frame(width: SampleWidth.form)
      .sampleSurface()
    }
  }
#endif
