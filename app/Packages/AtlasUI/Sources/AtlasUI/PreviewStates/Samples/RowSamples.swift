#if DEBUG
  import SwiftUI

  /// The rows' samples, one component at a time, at a window's width.
  struct RowSamples: View {
    /// Which component the samples show.
    enum Kind: Sendable {
      /// Waiting, current and done steps.
      case step
      /// A switch among menu rows, and a picker in a card.
      case control
      /// A selected symbol choice and an unselected letter choice.
      case choice
      /// A plain and a secure field.
      case field
      /// Done, busy and failed checks.
      case check
      /// A symbol tile with a status, and a text tile with a switch.
      case icon
      /// A setting with its switch.
      case setting
      /// A details group's facts and checks.
      case detail
    }

    private let kind: Kind

    /// Creates the samples of one component.
    ///
    /// - Parameter kind: Which component the samples show.
    init(_ kind: Kind) {
      self.kind = kind
    }

    /// That component's samples, on the ground.
    var body: some View {
      VStack(spacing: Spacing.s) {
        switch kind {
        case .step:
          StepRow(index: 1, title: String("Find Atlas in the list"), detail: String("Privacy & Security"), state: .done)
          StepRow(index: 2, title: String("Turn its switch on"), detail: String("Confirm it"), state: .current)
          StepRow(index: 3, title: String("Come back here"), detail: String("Atlas notices"), state: .waiting)
        case .control:
          ControlRow(size: .menu) {
            Text(verbatim: "Open at Login")
          } control: {
            Toggle(String("Open at Login"), isOn: .constant(true))
              .toggleStyle(.accentSwitch)
              .controlSize(.mini)
          }
          CardGroup {
            ControlRow {
              Text(verbatim: "Refresh")
                .foregroundStyle(Color.textSecondary)
            } control: {
              Picker(String("Refresh"), selection: .constant(2)) {
                Text(verbatim: "Every 2 seconds")
                  .tag(2)
              }
              .labelsHidden()
              .fixedSize()
            }
          }
        case .choice:
          ChoiceRow(
            title: String("Create a new account"),
            detail: String("Recommended."),
            mark: .symbol("plus"),
            selected: true
          ) {}
          ChoiceRow(
            title: String("Test Account"),
            detail: String("testaccount · standard account on this Mac"),
            mark: .letter("T"),
            selected: false
          ) {}
        case .field:
          CardGroup {
            FieldRow(label: String("Full name"), text: .constant(String("Atlas")))
            SeparatorLine()
            FieldRow(label: String("Password"), text: .constant(String("secret")), secure: true)
          }
        case .check:
          CardGroup {
            CheckRow(text: String("Atlas runs in the helper"), state: .done, note: String("starts by itself"))
            SeparatorLine()
            CheckRow(text: String("A test screenshot"), state: .busy, note: String("checking…"))
            SeparatorLine()
            CheckRow(text: String("A test screenshot"), state: .failed, note: String("failed"))
          }
        case .icon:
          CardGroup {
            IconRow(icon: .symbol("cursorarrow"), title: String("Accessibility"), detail: String("So agents can click"))
            {
              Text(verbatim: "Allowed")
                .font(Typography.font(.callout, weight: .semibold))
                .foregroundStyle(Color.textSuccess)
            }
            SeparatorLine()
            IconRow(icon: .text(">_"), title: String("Codex"), detail: String("Found.")) {
              Toggle(String("Codex"), isOn: .constant(true))
                .toggleStyle(.accentSwitch)
            }
          }
        case .setting:
          CardGroup {
            SettingRow(
              title: String("Open Atlas at login"),
              detail: String("So Atlas is there whenever you are."),
              isOn: .constant(true)
            )
          }
        case .detail:
          CardGroup(radius: Radius.medium) {
            DetailRow(name: String("Account"), value: String("Atlas"))
            SeparatorLine()
            DetailRow(name: String("Helper is running"), readiness: .ready)
            SeparatorLine()
            DetailRow(name: String("Save and Open dialogs"), readiness: .unknown, note: String("Not checked yet"))
            SeparatorLine()
            DetailRow(
              name: String("Mouse and keyboard"),
              readiness: .notReady,
              note: String("Mouse and keyboard aren't ready.")
            )
          }
          .frame(width: SampleWidth.narrow)
        }
      }
      .frame(width: SampleWidth.column)
      .sampleSurface()
    }
  }
#endif
