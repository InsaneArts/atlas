#if DEBUG
  import SwiftUI

  /// Recent's rows with the listed account each acted in beside its words and what let it act there, at the panel's
  /// width and at a narrow column, so a long line shows what gives way first.
  struct RecentRowSamples: View {
    /// Rows newest first: words naming only the browser, a short click on a site allowed for the task, a long one
    /// allowed once, a Trusted app, and one in no account.
    static let entries = [
      PanelRecentEntry(
        id: "1",
        icon: .app(.monogram(AppMonogram(name: "Google Chrome"))),
        line: "Clicked Merge in Google Chrome",
        when: "now",
        isLive: true,
        account: "github.com",
        allowance: PreviewStates.trustedAccount
      ),
      PanelRecentEntry(
        id: "2",
        icon: .app(.monogram(AppMonogram(name: "Google Chrome"))),
        line: "Clicked Comment",
        when: "9 s",
        account: "github.com",
        allowance: PreviewStates.allowedForTask
      ),
      PanelRecentEntry(
        id: "3",
        icon: .app(.monogram(AppMonogram(name: "Google Chrome"))),
        line: "Typed into Add a comment for the release notes",
        when: "31 s",
        account: "staging.acme.dev",
        allowance: PreviewStates.allowedOnce
      ),
      PanelRecentEntry(
        id: "4",
        icon: .app(.monogram(AppMonogram(name: "Slack"))),
        line: "Typed into Message",
        when: "1 min",
        account: "Slack",
        allowance: PreviewStates.trustedAccount
      ),
      PanelRecentEntry(
        id: "5",
        icon: .app(.monogram(AppMonogram(name: "TextEdit"))),
        line: "Clicked Save in TextEdit",
        when: "2 min"
      ),
    ]

    /// The rows at the panel's width, then in a narrow column.
    var body: some View {
      VStack(alignment: .leading, spacing: Spacing.l) {
        rows
          .frame(width: PanelLayout.width)
        rows
          .frame(width: SampleWidth.narrow)
      }
      .sampleSurface()
    }

    /// The rows, stacked as Recent lists them.
    private var rows: some View {
      VStack(spacing: .zero) {
        ForEach(Self.entries) { entry in
          PanelRecentRow(entry: entry)
        }
      }
    }
  }
#endif
